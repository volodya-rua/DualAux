#![cfg_attr(windows, windows_subsystem = "windows")]

pub struct Dev {
    pub id: String,
    pub label: String,
}

// Windows: mirror the default output's loopback to other devices (WASAPI via cpal).
#[cfg(windows)]
mod sys {
    use super::Dev;
    use cpal::traits::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    type Q = Arc<Mutex<VecDeque<[f32; 2]>>>;

    /// Longest per-output sync delay the UI offers.
    pub const MAX_DELAY_MS: u32 = 500;

    #[derive(Default)]
    pub struct Sys {
        streams: Vec<cpal::Stream>,
        vols: Vec<Arc<AtomicU32>>,
        delays: Vec<Arc<AtomicU32>>,
        /// Per output, for `--selftest`: loudest sample written (f32 bits) and ms until sound first arrived.
        pub peaks: Vec<Arc<AtomicU32>>,
        pub first: Vec<Arc<AtomicU64>>,
    }

    fn default_name() -> Option<String> {
        cpal::default_host().default_output_device()?.name().ok()
    }

    impl Sys {
        pub fn new() -> Self { Self::default() }
        /// Output 1 is whatever Windows plays on; it can't be chosen here.
        pub fn fixed_a(&self) -> Option<String> {
            Some(default_name().unwrap_or_else(|| "No default output".into()))
        }
        pub fn devices(&self) -> Vec<Dev> {
            let d = default_name();
            cpal::default_host()
                .output_devices()
                .map(|it| {
                    it.filter_map(|x| x.name().ok())
                        .filter(|n| Some(n) != d.as_ref())
                        .map(|n| Dev { id: n.clone(), label: n })
                        .collect()
                })
                .unwrap_or_default()
        }
        /// `delays` hold an output back by that many ms, to line up Bluetooth devices of different latency.
        pub fn start(&mut self, ids: &[String], vols: &[f32], delays: &[u32]) -> Result<(), String> {
            self.stop();
            let e = |e: &dyn std::fmt::Display| e.to_string();
            let host = cpal::default_host();
            let src = host.default_output_device().ok_or("No default output device")?;
            let all: Vec<_> = host.output_devices().map_err(|x| e(&x))?.collect();
            let ic = src.default_output_config().map_err(|x| e(&x))?;
            if ic.sample_format() != cpal::SampleFormat::F32 {
                return Err("Unsupported sample format (need 32-bit float)".into());
            }
            let (in_rate, in_ch) = (ic.sample_rate().0 as f64, ic.channels() as usize);
            self.vols = vols.iter().map(|v| Arc::new(AtomicU32::new(v.to_bits()))).collect();
            self.delays = delays.iter().map(|d| Arc::new(AtomicU32::new(*d))).collect();
            self.peaks = ids.iter().map(|_| Arc::new(AtomicU32::new(0))).collect();
            self.first = ids.iter().map(|_| Arc::new(AtomicU64::new(0))).collect();
            let t0 = Instant::now();

            // ponytail: Mutex in the audio callbacks; swap for a lock-free ring buffer if it glitches
            let qs: Vec<Q> = ids.iter().map(|_| Default::default()).collect();
            // room for the longest delay plus 250 ms of jitter; beyond that the oldest audio is dropped
            let (qi, cap) = (qs.clone(), (in_rate * (0.25 + MAX_DELAY_MS as f64 / 1000.0)) as usize);
            // building an *input* stream on an output device = WASAPI loopback
            let input = src
                .build_input_stream(
                    &ic.config(),
                    move |d: &[f32], _| {
                        for q in &qi {
                            let mut q = q.lock().unwrap();
                            for f in d.chunks(in_ch) {
                                q.push_back([f[0], *f.get(1).unwrap_or(&f[0])]);
                            }
                            while q.len() > cap {
                                q.pop_front();
                            }
                        }
                    },
                    |e| eprintln!("capture: {e}"),
                    None,
                )
                .map_err(|x| e(&x))?;

            let mut streams = vec![input];
            for (i, id) in ids.iter().enumerate() {
                let dst = all.iter().find(|d| d.name().ok().as_deref() == Some(id)).ok_or("Output not found")?;
                let oc = dst.default_output_config().map_err(|x| e(&x))?;
                if oc.sample_format() != cpal::SampleFormat::F32 {
                    return Err("Unsupported sample format (need 32-bit float)".into());
                }
                let (out_rate, out_ch) = (oc.sample_rate().0 as f64, oc.channels() as usize);
                let (q, vol, step) = (qs[i].clone(), self.vols[i].clone(), in_rate / out_rate);
                let (delay, peak, first) = (self.delays[i].clone(), self.peaks[i].clone(), self.first[i].clone());
                let (mut cur, mut nxt, mut phase) = ([0f32; 2], [0f32; 2], 1.0f64);
                let (mut primed, mut held) = (false, 0usize);
                let slack = (in_rate * 0.03) as usize; // jitter allowed before the hold-back is corrected
                let out = dst
                    .build_output_stream(
                        &oc.config(),
                        move |d: &mut [f32], _| {
                            let mut q = q.lock().unwrap();
                            let v = f32::from_bits(vol.load(Relaxed));
                            // Delay = how many captured frames to keep queued before playing. A longer delay refills
                            // the buffer (a short gap); a shorter one drops the extra frames.
                            let want = (delay.load(Relaxed) as f64 * in_rate / 1000.0) as usize;
                            if want > held + slack {
                                primed = false;
                            }
                            held = want;
                            if q.len() > want + slack {
                                let extra = q.len() - want - slack;
                                q.drain(..extra);
                            }
                            if !primed && q.len() >= want {
                                primed = true;
                            }
                            if !primed {
                                d.fill(0.0);
                                return;
                            }
                            for frame in d.chunks_mut(out_ch) {
                                while phase >= 1.0 {
                                    match q.pop_front() {
                                        Some(f) => { cur = nxt; nxt = f; phase -= 1.0 }
                                        None => break,
                                    }
                                }
                                if phase >= 1.0 {
                                    frame.fill(0.0); // underrun
                                    continue;
                                }
                                let t = phase as f32;
                                for (i, s) in frame.iter_mut().enumerate() {
                                    let c = i & 1;
                                    *s = if i < 2 { (cur[c] + (nxt[c] - cur[c]) * t) * v } else { 0.0 };
                                }
                                phase += step;
                            }
                            if q.is_empty() && want > 0 {
                                primed = false; // ran dry: rebuild the hold-back
                            }
                            let m = d.iter().fold(0f32, |a, &s| a.max(s.abs()));
                            peak.fetch_max(m.to_bits(), Relaxed);
                            if m > 0.01 && first.load(Relaxed) == 0 {
                                first.store((t0.elapsed().as_millis() as u64).max(1), Relaxed);
                            }
                        },
                        |e| eprintln!("playback: {e}"),
                        None,
                    )
                    .map_err(|x| e(&x))?;
                streams.push(out);
            }
            for s in &streams {
                s.play().map_err(|x| e(&x))?;
            }
            self.streams = streams;
            Ok(())
        }
        pub fn stop(&mut self) {
            self.streams.clear();
        }
        pub fn vol(&self, i: usize, _id: &str, v: f32) {
            if let Some(a) = self.vols.get(i) {
                a.store(v.to_bits(), Relaxed);
            }
        }
        pub fn delay(&self, i: usize, ms: u32) {
            if let Some(a) = self.delays.get(i) {
                a.store(ms.min(MAX_DELAY_MS), Relaxed);
            }
        }
    }
}

// Linux: PulseAudio/PipeWire combined sink. macOS: stacked aggregate device via a Swift helper.
#[cfg(unix)]
mod sys {
    use super::Dev;
    use std::process::Command;

    fn sh(cmd: &str, args: &[&str]) -> Result<String, String> {
        let o = Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
        }
    }

    #[cfg(target_os = "macos")]
    fn mac(args: &[&str]) -> Result<String, String> {
        let dir = std::env::temp_dir();
        let bin = dir.join("dualaux-mac");
        if !bin.exists() {
            let src = dir.join("dualaux-mac.swift");
            std::fs::write(&src, include_str!("../mac.swift")).map_err(|e| e.to_string())?;
            sh("swiftc", &["-O", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])?; // needs Xcode CLT
        }
        sh(bin.to_str().unwrap(), args)
    }

    #[derive(Default)]
    pub struct Sys {
        module: Option<String>,
        prev: Option<String>,
    }

    impl Sys {
        pub fn new() -> Self { Self::default() }
        pub fn fixed_a(&self) -> Option<String> { None }
        pub fn devices(&self) -> Vec<Dev> {
            #[cfg(target_os = "linux")]
            let (raw, id, label) = (sh("pactl", &["-f", "json", "list", "sinks"]), "name", "description");
            #[cfg(target_os = "macos")]
            let (raw, id, label) = (mac(&["list"]), "id", "label");
            let v: serde_json::Value = serde_json::from_str(&raw.unwrap_or_default()).unwrap_or_default();
            v.as_array().into_iter().flatten()
                .filter(|s| s[id] != "dualaux")
                .map(|s| Dev { id: s[id].as_str().unwrap_or("").into(), label: s[label].as_str().unwrap_or("").into() })
                .collect()
        }
        pub fn start(&mut self, ids: &[String], vols: &[f32], _delays: &[u32]) -> Result<(), String> {
            #[cfg(target_os = "linux")]
            {
                self.prev = Some(sh("pactl", &["get-default-sink"])?);
                self.module = Some(sh("pactl", &["load-module", "module-combine-sink", "sink_name=dualaux", &format!("slaves={}", ids.join(","))])?);
                sh("pactl", &["set-default-sink", "dualaux"])?;
            }
            #[cfg(target_os = "macos")]
            {
                let mut a = vec!["start"];
                a.extend(ids.iter().map(|s| s.as_str()));
                mac(&a)?;
            }
            for (i, id) in ids.iter().enumerate() {
                self.vol(i, id, vols[i]);
            }
            Ok(())
        }
        pub fn stop(&mut self) {
            #[cfg(target_os = "linux")]
            {
                if let Some(p) = self.prev.take() { let _ = sh("pactl", &["set-default-sink", &p]); }
                if let Some(m) = self.module.take() { let _ = sh("pactl", &["unload-module", &m]); }
            }
            #[cfg(target_os = "macos")]
            let _ = mac(&["stop"]);
        }
        pub fn delay(&self, _i: usize, _ms: u32) {} // sync delay is Windows-only for now
        pub fn vol(&self, _i: usize, id: &str, v: f32) {
            #[cfg(target_os = "linux")]
            let _ = sh("pactl", &["set-sink-volume", id, &format!("{}%", (v * 100.0).round())]);
            #[cfg(target_os = "macos")]
            let _ = mac(&["vol", id, &v.to_string()]);
        }
    }
}


mod ui;

/// `dualaux --selftest <outfile>`: play a tone on the default output, mirror it to every other
/// output for 3 s, and write what reached each one. Lets the audio path be checked without ears.
#[cfg(windows)]
fn selftest(out: &str) -> String {
    use cpal::traits::*;
    use std::sync::atomic::Ordering::Relaxed;
    let mut sys = sys::Sys::new();
    let ids: Vec<_> = sys.devices().into_iter().map(|d| d.id).collect();
    if ids.is_empty() {
        return "SKIP: no second output device".into();
    }
    let host = cpal::default_host();
    let Some(dev) = host.default_output_device() else { return "FAIL: no default device".into() };
    let cfg = dev.default_output_config().unwrap();
    let (rate, ch) = (cfg.sample_rate().0 as f32, cfg.channels() as usize);
    let mut t = 0f32;
    let tone = dev
        .build_output_stream(
            &cfg.config(),
            move |d: &mut [f32], _| {
                for f in d.chunks_mut(ch) {
                    t += 440.0 / rate;
                    f.fill((t * std::f32::consts::TAU).sin() * 0.5);
                }
            },
            |_| {},
            None,
        )
        .unwrap();
    tone.play().unwrap();
    // Five outputs in total (the default plus four mirrors). With fewer real devices the same one is reused,
    // which still exercises four parallel streams. The last mirror is held back 200 ms to check the delay.
    let ids: Vec<String> = (0..4).map(|i| ids[i % ids.len()].clone()).collect();
    let vols = vec![1.0; 4];
    let delays = [0, 0, 0, 200];
    if let Err(e) = sys.start(&ids, &vols, &delays) {
        return format!("FAIL: start: {e}");
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    let peaks: Vec<f32> = sys.peaks.iter().map(|p| f32::from_bits(p.load(Relaxed))).collect();
    let first: Vec<u64> = sys.first.iter().map(|f| f.load(Relaxed)).collect();
    sys.stop();
    let _ = out;
    let loud = peaks.iter().all(|&p| p > 0.05);
    let gap = first[3] as i64 - first[0] as i64;
    let delayed = (150..=400).contains(&gap);
    format!(
        "4 mirrored outputs {:?}\npeaks {:?}\nfirst sound ms {:?}, delayed output arrives {} ms later\n-> {}",
        ids, peaks, first, gap,
        if loud && delayed { "PASS" } else if !loud { "FAIL (silent output)" } else { "FAIL (delay off)" }
    )
}

/// Linux CI check: combine the first two sinks, confirm it became the default, then confirm Stop restores it.
#[cfg(target_os = "linux")]
fn selftest() -> Result<String, String> {
    let sh = |a: &[&str]| -> String {
        let o = std::process::Command::new("pactl").args(a).output().map(|o| o.stdout).unwrap_or_default();
        String::from_utf8_lossy(&o).trim().to_string()
    };
    let mut sys = sys::Sys::new();
    let ids: Vec<_> = sys.devices().into_iter().map(|d| d.id).take(2).collect();
    if ids.len() < 2 {
        return Err("need two sinks".into());
    }
    let before = sh(&["get-default-sink"]);
    sys.start(&ids, &[1.0, 0.5], &[0, 0])?;
    let during = sh(&["get-default-sink"]);
    let slaves = sh(&["list", "short", "sinks"]);
    sys.stop();
    let after = sh(&["get-default-sink"]);
    let ok = during == "dualaux" && slaves.contains("dualaux") && after == before && !sh(&["list", "short", "sinks"]).contains("dualaux");
    let msg = format!("sinks {ids:?}: before={before} during={during} after={after}");
    if ok { Ok(format!("PASS {msg}")) } else { Err(format!("FAIL {msg}")) }
}

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--selftest") {
        match selftest() {
            Ok(m) => println!("{m}"),
            Err(m) => { eprintln!("{m}"); std::process::exit(1) }
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        let a: Vec<String> = std::env::args().collect();
        if a.get(1).map(|s| s.as_str()) == Some("--selftest") {
            let out = a.get(2).cloned().unwrap_or_default();
            let _ = std::fs::write(&out, selftest(&out));
            return Ok(());
        }
    }
    ui::run()
}
