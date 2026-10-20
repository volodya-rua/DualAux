# Privacy Policy

_Last updated: 2026-10-19_

**DualAux does not collect, store, transmit, sell or share any personal data. Not even anonymous or "diagnostic" data.**

## The short version

- No accounts, no sign-in, no ads, no analytics, no crash reporting, no cookies, no tracking.
- The app cannot send data off your device: the desktop app contains no networking code, and the Android app does not request the `INTERNET` permission.
- Everything happens on your device and in memory. Nothing is written to disk or uploaded.

## What the app touches, and why

| What | Why | What happens to it |
|---|---|---|
| Names and IDs of your audio output devices | To show the device list | Read locally, shown on screen, discarded when you close the app |
| The audio your device is playing | To play it on more than one output | Passed straight from memory to your chosen outputs. Never saved, recorded to a file, analyzed or sent anywhere |
| Volume sliders and device selection | To control playback | Held in memory only; not saved between launches |

### Android permissions

- **Record audio (`RECORD_AUDIO`)**: required by Android to capture the audio your phone is playing (AudioPlaybackCapture). The microphone is **not** used.
- **Screen capture prompt (MediaProjection)**: Android shows this prompt for audio capture. DualAux requests **audio only** and never captures your screen.
- **Foreground service and notifications**: to keep playing with the screen off and to show the "mirroring" notification.
- **Nearby devices (`BLUETOOTH_CONNECT`)**: only to read the names of Bluetooth headphones that are already connected, so DualAux can show the ones Android won't let apps play to. Android's prompt mentions finding devices and their position because that is its standard wording for this permission; DualAux never scans, pairs, connects or uses location. You can deny it and the app still works, without that list.

### Desktop

- **Windows**: reads the default output device's audio (WASAPI loopback) and plays it to the other devices you pick. A tray icon lets the app run in the background; it collects nothing.
- **Linux**: runs the `pactl` command to create a combined audio output, and removes it when you press Stop.
- **macOS**: compiles and runs a small helper (`mac.swift`) in your temporary folder to create a multi-output device, and removes it when you press Stop.

## Third parties

DualAux has no third-party services built in. The source code and releases are hosted on GitHub, which has its own [privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement); that applies to your use of GitHub, not to running DualAux.

## Children

DualAux collects no data from anyone, including children.

## Changes

Any change to this policy will be made in this file and visible in the repository's history.

## Contact

Questions: open an issue at <https://github.com/volodya-rua/DualAux/issues>.
