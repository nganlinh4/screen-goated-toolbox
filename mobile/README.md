# SGT Mobile

Android-first live translate app that lives beside the Windows desktop app in the same repo.

## Structure

- `androidApp/`: native Android app built with Kotlin and Jetpack Compose
- `shared/`: Kotlin Multiplatform session contracts and state store

## Current v1 focus

- Material 3 Android UI
- `mic` and `playback` live capture modes
- foreground-service driven live translate runtime
- optional floating overlay in the `full` flavor
- BYOK Gemini credentials stored locally on device

## Flavors

- `full`: direct APK distribution with overlay support enabled
- `play`: Play-safe flavor with overlay support disabled at build time

## Run

1. Install a Java 17+ JDK.
2. From `mobile/`, run `./gradlew :androidApp:assembleFullDebug`.
3. Open the generated APK on an Android device or emulator that supports playback capture.

## WSL Install

If you work from WSL, use the repo wrapper instead of WSL-native Gradle:

```bash
./mobile/scripts/sgtp-wsl.sh install
```

That wrapper runs the known-good Windows JDK/Android SDK/Gradle toolchain through `powershell.exe`, then installs the APK with the existing `sgtp` phone helper. It is the supported stress-free path from WSL for this repo.

## Phone Helper

Use the single Windows command `sgtp`.

- `sgtp`: connect, open filtered logs, install, launch
- `sgtp pair`: one-time wireless debugging pair if trust was lost

The helper reads `mobile/.sgtp.json`, which is ignored by git.
If Android wireless debugging discovery works, `sgtp` can rediscover the current endpoint automatically.
If discovery is blocked on your network, `sgtp` falls back to the phone's current `IP address & Port`.
`enable-fixed-port` still exists, but it is not reliable across phone reboots.

WSL can use the same flow through:

- `./mobile/scripts/sgtp-wsl.sh build`
- `./mobile/scripts/sgtp-wsl.sh install`
- `./mobile/scripts/sgtp-wsl.sh run`
- `./mobile/scripts/sgtp-wsl.sh status`
