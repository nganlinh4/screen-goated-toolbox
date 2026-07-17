# SGT Mobile

Android/Kotlin Multiplatform companion for Screen Goated Toolbox. Windows remains canonical for features covered by `.claude/parity/`; Android ports those contracts into native Compose and Android services.

## Layout

- `androidApp/` — Android application, Compose UI, services, overlays, and platform integrations.
- `shared/` — shared session/state contracts.
- `scripts/` — Windows/WSL Gradle and ADB helpers.
- `build-release.ps1` — signed full APK and optional Play AAB wrapper.

## Flavors

- `full` — direct/development distribution with floating-overlay features.
- `play` — Google Play distribution with Play-safe feature switches and in-app updates.

Both derive `versionName` and default `versionCode` from root `Cargo.toml`.

## Prerequisites

- JDK 17.
- Android SDK and platform tools.
- Android platform/build tools required by `androidApp/build.gradle.kts`.
- For this checkout, `settings.gradle.kts` includes the `../../youtubedl-android` composite build; that repository must exist at the expected path relative to `mobile/`.

Set `JAVA_HOME`, `ANDROID_HOME`, and `ANDROID_SDK_ROOT` for the local installation. Keep machine paths in untracked local configuration, not this README.

## Build and test on Windows

From `mobile/`:

```powershell
.\gradlew.bat :androidApp:assembleFullDebug --console=plain
.\gradlew.bat :androidApp:testFullDebugUnitTest --console=plain
.\gradlew.bat :androidApp:compileFullDebugKotlin --console=plain
```

Generated debug APK:

`androidApp/build/outputs/apk/full/debug/androidApp-full-debug.apk`

## WSL wrapper

WSL-native Gradle is unreliable with the Windows Android SDK in this environment. Delegate to PowerShell:

```bash
./mobile/scripts/sgtp-wsl.sh build
./mobile/scripts/sgtp-wsl.sh install
./mobile/scripts/sgtp-wsl.sh run
./mobile/scripts/sgtp-wsl.sh status
./mobile/scripts/sgtp-wsl.sh gradle :androidApp:testFullDebugUnitTest --console=plain
```

Set `SGT_REPO_ROOT` only when the wrapper cannot discover the checkout.

## Device helper

`scripts/sgtp.ps1` wraps wireless ADB connection, install, launch, and filtered logs. It reads `mobile/.sgtp.json`; update that machine/device-specific file when endpoint or APK location changes.

```powershell
.\scripts\sgtp.ps1 status
.\scripts\sgtp.ps1 pair
.\scripts\sgtp.ps1 install
.\scripts\sgtp.ps1 run
.\scripts\sgtp.ps1 logcat
```

Wireless-debugging ports may change after reconnect/reboot. The helper attempts endpoint discovery and falls back to saved host/port.

## Release artifacts

From repo root:

```powershell
# Full signed APK only
.\mobile\build-release.ps1

# Full APK + Play AAB
.\mobile\build-release.ps1 -IncludeAab
```

Copied outputs:

- `target/release/ScreenGoatedToolbox_v<VERSION>.apk`
- `target/release/ScreenGoatedToolbox_v<VERSION>.aab` when `-IncludeAab` is used.

## Play on-demand native delivery

The `play` bundle keeps executable native payloads out of the base module. Google Play delivers
the ASR engines, downloader tools, and their shared C++ runtime through the on-demand dynamic
features under `feature_*`. The `full` flavor keeps its existing direct-download implementation;
feature modules are not fused into its standalone APK.

After building the Play release bundle, verify its native ownership and updater strings with:

```powershell
.\gradlew.bat :androidApp:verifyPlayReleaseCompliance --console=plain
```

For a same-version Play re-upload, Gradle supports `-PversionCodeOverride=<INT>`; invoke the relevant Gradle bundle task directly.

## Parity workflow

Before changing a parity-owned feature:

1. Read `.claude/skills/enforce-mobile-parity/SKILL.md`.
2. Update its `.claude/parity/<feature>.md` contract.
3. Update shared fixtures under `parity-fixtures/`.
4. Implement against Windows behavior.
5. Run Windows and Android fixture/tests named by the parity contract.
