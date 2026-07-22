# SGT Mobile

Android/Kotlin Multiplatform companion for Screen Goated Toolbox. Windows remains canonical for features covered by `.claude/parity/`; Android ports those contracts into native Compose and Android services.

## Layout

- `androidApp/` — Android application, Compose UI, services, overlays, and platform integrations.
- `shared/` — shared session/state contracts.
- `scripts/` — Windows/WSL Gradle and ADB helpers.
- `build-release.ps1` — signed full APK and optional Play AAB wrapper.

## Flavors

- `full` — direct/development distribution artifact.
- `play` — store-distribution artifact with in-app update delivery.

Phone Control is the same product in both distributions: its entry point, optional
user-granted overlay, stable tool catalog, runtime, Accessibility backend, and
Shizuku/root authority are identical. Distribution-specific native delivery changes
provider readiness mechanics, not the catalog or authority contract.

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
.\gradlew.bat :androidApp:assembleFullDebug :androidApp:assemblePlayDebug --console=plain
.\gradlew.bat :androidApp:testFullDebugUnitTest :androidApp:testPlayDebugUnitTest --console=plain
.\gradlew.bat :androidApp:compileFullDebugKotlin :androidApp:compilePlayDebugKotlin --console=plain
```

Generated debug APKs:

- `androidApp/build/outputs/apk/full/debug/androidApp-full-debug.apk`
- `androidApp/build/outputs/apk/play/debug/androidApp-play-debug.apk`

## WSL wrapper

WSL-native Gradle is unreliable with the Windows Android SDK in this environment. Delegate to PowerShell:

```bash
./mobile/scripts/sgtp-wsl.sh build
./mobile/scripts/sgtp-wsl.sh install
./mobile/scripts/sgtp-wsl.sh run
./mobile/scripts/sgtp-wsl.sh status
./mobile/scripts/sgtp-wsl.sh gradle :androidApp:testFullDebugUnitTest :androidApp:testPlayDebugUnitTest --console=plain
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

The Phone Control device harness builds and exercises both distributions by default:

```powershell
.\scripts\run-phone-control-tests.ps1 -Flavor Both
```

Phone Control also keeps a bounded, privacy-safe structural event journal so a
real session can be diagnosed after the fact. Collect it from one exact device
and package without touching other Android users:

```powershell
.\scripts\collect-phone-control-diagnostics.ps1 -Serial 47311FDAQ002H3 -Variant Release
```

Use `-Variant Debug` for the journaled test package. The collector writes the
current/previous JSONL journal, filtered Phone Control Logcat, and capture
metadata to a timestamped directory beside the script. It never collects
transcripts, screen/node text, screenshots, clipboard/file/browser content,
command output, API keys, or authentication material.

The harness accepts `-Serial`, requires a verified emulator unless
`-AllowPhysicalDevice` is explicit, and binds every ADB operation to that exact
serial. A run refuses to replace a pre-existing debug or instrumentation package,
including on an emulator. It journals device identity and original Accessibility,
overlay, foreground, keep-awake, and release-package state under the ignored mobile build
directory; interrupted runs recover that state before another run. Normal exit
removes only debug/test packages installed by that run and verifies that the
release version, update time, split paths, and APK hashes did not change. Its Play
path installs a bundletool local-testing AAB so the test can request real
on-demand modules. The user-scoped installer keeps other Android users untouched
while reproducing bundletool's local-testing contract: install the base splits,
stage every non-base-master split in the declared directory, and finalize that
directory as app-readable. The acceptance test proves the Play ORT runtime and
bundled UI-DETR model by running inference on the current device frame.

Pass `-IncludeExternalSetupTests` when the selected device may open Android-owned
setup surfaces. The Shizuku probe then verifies the real install handoff when the
package is absent; the harness still restores the original foreground app and
device state afterward.

Use `scripts/invoke-phone-control-probe.ps1` only against an installed debug
package. Physical targets require `-AllowPhysicalDevice`; registry-classified
mutating tools additionally require `-AllowMutation`. Probe receipts are
request-scoped and removed after cancellation or completion.

For several real probes against one clean debug install without running or
claiming the instrumentation acceptance suite, prepare exactly one flavor in the
same durable recovery journal, then restore it explicitly:

```powershell
.\scripts\run-phone-control-tests.ps1 -Flavor Full -Serial <serial> -AllowPhysicalDevice -PrepareDebugForProbes
# invoke-phone-control-probe.ps1 calls...
.\scripts\run-phone-control-tests.ps1 -Serial <serial> -AllowPhysicalDevice -RestoreOnly
```

Preparation assembles and installs only the selected debug app, enables and
verifies its Accessibility service, records package ownership and recoverable
device state, keeps an unlocked target awake for the journaled session, and
attests that the release package paths did not change. Restore returns the exact
prior keep-awake setting. Preparation does not assemble or install the AndroidTest
package and does not imply acceptance.

To run the acceptance suite first and retain its selected debug app afterward,
use the separate retained-test mode:

```powershell
.\scripts\run-phone-control-tests.ps1 -Flavor Full -Serial <serial> -AllowPhysicalDevice -RetainDebugForProbes
# invoke-phone-control-probe.ps1 calls...
.\scripts\run-phone-control-tests.ps1 -Serial <serial> -AllowPhysicalDevice -RestoreOnly
```

Both modes keep only the run-owned debug app and bound Accessibility service and
leave the original release app untouched; retained-test mode first removes its
instrumentation APK. A later harness invocation recovers the same journal before
doing any new work if the explicit restore command was interrupted.

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

The `play` bundle keeps large native payloads out of the base module and delivers
the ASR/ORT engines and shared C++ runtime through on-demand dynamic features
under `feature_*`. Its ORT feature also carries the validated UI-DETR model. The
`full` artifact bundles the exact verified ORT archive and uses verified downloads
for the UI-DETR model and remaining native runtimes. This delivery difference does
not change Phone Control's catalog, runtime, provider routes, or authority.

Build or install a locally deliverable Play debug bundle with:

```powershell
.\gradlew.bat :androidApp:buildPlayDebugLocalTestingApks -PphoneControlDeviceSerial=emulator-5554 --console=plain
.\gradlew.bat :androidApp:installPlayDebugLocalTesting -PphoneControlDeviceSerial=emulator-5554 --console=plain
```

The serial is mandatory for install and scopes the generated APK set. BundleTool
refreshes the exact device specification before each targeted build, so a reused
serial cannot silently reuse splits for a different ABI, density, locale, or SDK.

After building the Play release bundle, verify native/model ownership, exact
model bytes, the private confirmation proxy, and updater strings with:

```powershell
.\gradlew.bat :androidApp:verifyPlayReleaseCompliance --console=plain
```

That task verifies repository packaging invariants only; it does not predict or assert store
review acceptance.

For a same-version Play re-upload, Gradle supports `-PversionCodeOverride=<INT>`; invoke the relevant Gradle bundle task directly.

## Parity workflow

Before changing a parity-owned feature:

1. Read `.claude/skills/enforce-mobile-parity/SKILL.md`.
2. Update its `.claude/parity/<feature>.md` contract.
3. Update shared fixtures under `parity-fixtures/`.
4. Implement against Windows behavior.
5. Run Windows and Android fixture/tests named by the parity contract.
