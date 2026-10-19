# DualAux

Play your audio on two to five headphones, earphones or speakers at once: wired, wireless, or a mix. Minimal rich-black UI. MIT licensed.

| Platform | How it works | Status |
|---|---|---|
| Windows | Mirrors the default output (WASAPI loopback) to the other devices you pick. Closing the window hides it in the tray. | Works. Automated audio self-test passes on real hardware |
| Linux | Creates a PulseAudio/PipeWire combined sink (`pactl`). | CI self-test passes with virtual sinks; not yet tried with real headphones |
| macOS | Creates a stacked CoreAudio multi-output device (`mac.swift`, compiled on first run; needs Xcode CLT). | Builds in CI; not yet run on a Mac |
| Android | Captures media audio (AudioPlaybackCapture) and plays it to each chosen output. Android 10+. | Runs on an emulator (start/stop, capture prompt); not yet tried with real headphones |
| iOS | Not possible: iOS allows one audio route per app. | n/a |

## Limits

- **Windows:** Output 1 is whatever Windows plays on; the others are mirrored from it. Bluetooth devices lag behind wired ones, and there is no per-device delay yet.
- **Android:** when Android asks what to record or cast, choose **Entire screen** (not "A single app") so all audio is captured. DualAux never records your screen. The original audio still plays on the phone's default route, so one device may sound doubled. Most phones can't hold two Bluetooth A2DP headsets at once; wired + Bluetooth works best. Apps that block capture (e.g. DRM video) can't be mirrored.

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
