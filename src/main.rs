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
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
    use std::sync::{Arc, Mutex};

    type Q = Arc<Mutex<VecDeque<[f32; 2]>>>;

    #[derive(Default)]
    pub struct Sys {
        streams: Vec<cpal::Stream>,
        vols: Vec<Arc<AtomicU32>>,
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
        pub fn start(&mut self, ids: &[String], vols: &[f32]) -> Result<(), String> {
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

            // ponytail: Mutex in the audio callbacks; swap for a lock-free ring buffer if it glitches
            let qs: Vec<Q> = ids.iter().map(|_| Default::default()).collect();
            let (qi, cap) = (qs.clone(), (in_rate * 0.25) as usize); // cap latency at 250 ms
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
                let (mut cur, mut nxt, mut phase) = ([0f32; 2], [0f32; 2], 1.0f64);
                let out = dst
                    .build_output_stream(
                        &oc.config(),
                        move |d: &mut [f32], _| {
                            let mut q = q.lock().unwrap();
                            let v = f32::from_bits(vol.load(Relaxed));
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
        pub fn start(&mut self, ids: &[String], vols: &[f32]) -> Result<(), String> {
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
        pub fn vol(&self, _i: usize, id: &str, v: f32) {
            #[cfg(target_os = "linux")]
            let _ = sh("pactl", &["set-sink-volume", id, &format!("{}%", (v * 100.0).round())]);
            #[cfg(target_os = "macos")]
            let _ = mac(&["vol", id, &v.to_string()]);
        }
    }
}


mod ui;

fn main() -> eframe::Result {
    ui::run()
}
