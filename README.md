# DualAux

Play your audio on two to five headphones, earphones or speakers at once: wired, wireless, or a mix. Minimal rich-black UI. MIT licensed.

| Platform | How it works | Status |
|---|---|---|
| Windows | The 5-device hub: mirrors the default output (WASAPI loopback) to up to 4 more outputs, each with its own volume and sync delay (0-500 ms) to line up Bluetooth devices of different latency. Closing the window hides it in the tray. | Works. Automated test passes on real hardware: 5 outputs, every one gets audio, 200 ms delay measured at 193 ms |
| Linux | Creates a PulseAudio/PipeWire combined sink (`pactl`). | CI self-test passes with virtual sinks; not yet tried with real headphones |
| macOS | Creates a stacked CoreAudio multi-output device (`mac.swift`, compiled on first run; needs Xcode CLT). | Builds in CI; not yet run on a Mac |
| Android | Captures media audio (AudioPlaybackCapture) and plays it to each chosen output. Android 10+. | Runs on an emulator (start/stop, capture prompt); not yet tried with real headphones |
| iOS | Not possible: iOS allows one audio route per app. | n/a |

## Limits

- **Windows:** Output 1 is whatever Windows plays on and can't be delayed, so set the slowest device (usually Bluetooth) as the Windows default and use **Sync delay** on the others. Windows itself must be able to connect all your headphones at once (a USB Bluetooth adapter helps).
- **Android:** when Android asks what to record or cast, choose **Entire screen** (not "A single app") so all audio is captured. DualAux never records your screen. The original audio still plays on the phone's default route, so one device may sound doubled. **How many outputs really play at once is decided by your phone's Bluetooth stack, not by DualAux:** stock Android sends audio to only one Bluetooth headset at a time, so several Bluetooth headsets only work on phones with a dual-audio / multi-connection feature, or with LE Audio devices on newer Android. Wired, USB and one Bluetooth device normally work together. Each card shows whether Android really routed audio to that device ("Playing", or a warning), and Bluetooth headphones that are connected but held back by Android are listed with an explanation. For up to 5 Bluetooth headphones, use the Windows app. Apps that block capture (e.g. DRM video) can't be mirrored.

## Build

Desktop (Rust):

```bash
cargo run --release
```

Linux needs `libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev`. CI builds all three desktops (see `.github/workflows/build.yml`).

Android (JDK 17 + Android SDK):

```bash
cd android && ./gradlew assembleDebug
```

## Privacy and legal

DualAux collects nothing and has no network code: see [PRIVACY.md](PRIVACY.md). Also see [DISCLAIMER.md](DISCLAIMER.md), [SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md) and [third-party licenses](THIRD_PARTY_LICENSES.md).

## License

[MIT](LICENSE)
