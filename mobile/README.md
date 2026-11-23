# SGT Mobile

Android/Kotlin Multiplatform companion for Screen Goated Toolbox. Windows remains canonical for features covered by `.claude/parity/`; Android ports those contracts into native Compose and Android services.

## Phone Control change gate

`AGENTS.md` is the mandatory Android change gate. It routes Phone Control work
to the canonical Windows contract, Android parity contract, parity workflow, and
shared fixtures. Do not restate those contracts here.

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
first-party SGT Bridge/Shizuku/root authority are identical. Distribution-specific
native delivery changes provider readiness mechanics, not the catalog or authority
contract.

Both derive `versionName` and default `versionCode` from root `Cargo.toml`.

The Full downloader keeps Android's native executable launchers in the APK.
Python, FFmpeg, and the bootstrap yt-dlp install on first use from the verified
`androidApp/delivery/downloader-runtime.json` contract. The app checks for
official upstream stable yt-dlp updates independently. It verifies the tagged
release's signed checksum and probes the candidate with the Android launcher
before activation. Downloaded Tools shows the active version and offers a
manual check. Active jobs retain their executable, and a failed update leaves
the previous installation usable.

## Prerequisites

- JDK 21 (matching `gradle/gradle-daemon-jvm.properties`).
- Python 3 with the Windows `py` launcher for Gradle's source/contract generators.
- Android SDK and platform tools.
- Android platform/build tools required by `androidApp/build.gradle.kts`.

Set `JAVA_HOME`, `ANDROID_HOME`, and `ANDROID_SDK_ROOT` for the local installation. Keep machine paths in untracked local configuration, not this README.

## Android Video Downloader development

The Full flavor uses SGT's public [youtubedl-android repository](https://github.com/nganlinh4/youtubedl-android).
Run the clone command from the directory containing the SGT checkout.

```sh
git clone https://github.com/nganlinh4/youtubedl-android.git
```

```text
<any parent folder>/
  screen-goated-toolbox/
    mobile/
  youtubedl-android/
```

Gradle uses the sibling checkout for both the composite build and the bundled
native launchers. To reproduce CI locally, use the commit pinned in the
[Android CI workflow](../.github/workflows/development.yml).

The [Android agent rules](AGENTS.md#owned-downloader-dependency) define ownership
and push authority. For a fix in the shared library, follow these steps.

1. Implement and verify the fix in `youtubedl-android`, then commit and push it to that repository's `origin`.
2. Update the Android CI checkout pin to the pushed commit. If any native launcher bytes changed, update their sizes and SHA-256 hashes in the [launcher contract](androidApp/gradle/runtime-delivery.gradle.kts).
3. With the sibling checkout at that commit, run the [Full unit tests and debug build](#build-and-test-on-windows).

## Build and test on Windows

From `mobile/`, run the affected flavor while iterating:

```powershell
# Shared or Full-only code
.\gradlew.bat :androidApp:testFullDebugUnitTest --console=plain

# Play delivery code
.\gradlew.bat :androidApp:testPlayDebugUnitTest --console=plain

# Produce only the APK currently needed for device testing
.\gradlew.bat :androidApp:assembleFullDebug --console=plain
```

The unit and assemble tasks already compile their selected variants. Do not
follow them with explicit `compileFullDebugKotlin` or
`compilePlayDebugKotlin` tasks. Before a cross-flavor checkpoint or release,
run both unit suites together; assemble both debug APKs only when both
artifacts are actually needed:

```powershell
.\gradlew.bat :androidApp:testFullDebugUnitTest :androidApp:testPlayDebugUnitTest --console=plain
.\gradlew.bat :androidApp:assembleFullDebug :androidApp:assemblePlayDebug --console=plain
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

`scripts/sgtp.ps1` wraps wireless ADB connection, install, launch, and filtered logs. It keeps the device endpoint and APK location in `%LOCALAPPDATA%\SGT-Development\sgtp.json` (or `SGT_SGTP_CONFIG`), outside the checkout; an older `mobile/.sgtp.json` is read once as a migration source.

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
.\scripts\run-phone-control-tests.ps1 -Flavor Both -Serial <serial>
```

Phone Control also keeps a bounded, privacy-safe structural event journal so a
real session can be diagnosed after the fact. Collect it from one exact device
and package without touching other Android users:

```powershell
.\scripts\collect-phone-control-diagnostics.ps1 -Serial <serial> -Variant Release
```

Use `-Variant Debug` for the journaled test package. The collector writes the
current/previous JSONL journal, a compact `timeline.txt` and `summary.json`,
filtered Phone Control Logcat, and capture metadata to a timestamped directory
beside the script. Start diagnosis with the summary and timeline; keep the raw
journal as evidence. It never collects
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
directory as app-readable. Phone Control grounding uses the shared current-frame
Gemini chain in both flavors, so it has no flavor-specific visual ML asset.

Pass `-IncludeExternalSetupTests` when the selected device may open Android-owned
setup surfaces. The Shizuku probe then verifies the real install handoff when the
package is absent; the harness still restores the original foreground app and
device state afterward.

The optional **SGT Bridge** provider is built into both distributions. It is
currently unavailable because its wireless command endpoint cannot be verified.
Other authority choices remain available. It keeps
its ADB private key non-exportable in Android Keystore, discovers only the
device-local wireless-debugging services, and runs its bounded command client in
a private app process; it never opens a LAN command listener. The compact orb
prompt also exposes an explicit pairing-forget action. Its transport uses
`libadb-android` 3.1.1 with Android's platform TLS 1.3 provider (the app's
minimum API level is 29). Upstream states that libadb has not
received a security audit, so the dependency threat model remains an explicit
release-review item even though real-device pairing, reconnect, cancellation,
revoke, projection-resume, and credentialed Chrome CDP target
bind/read/verified-close checks do not establish server identity.

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

Before publishing a Full build whose downloader manifest changes, prepare the
two custom archives and upload the printed, uniquely named files without
replacing an existing runtime-bundles asset:

```powershell
.\mobile\scripts\prepare-downloader-runtime-bundles.ps1
```

The script verifies its source archives against the build manifest before it
copies anything. Full asset merging fails when the delivery manifest is absent
or violates the immutable URL/identity contract.

## Play on-demand delivery

The `play` bundle keeps the large creation runtime out of the base module in the
on-demand `feature_creation_runtime` module. Android ships no offline speech
engine; `docs/OFFLINE-ENGINES.md` describes where one would plug in. Phone Control has no
flavor-specific grounding runtime; both distributions use the same current-frame
vision contract, catalog, provider routes, and authority.

The Image-to-3D result viewer is built from the canonical Windows viewer source
with `npm run build:viewer` in `3d-generator-ui/`. Its deterministic
`viewer-dist/creation_model_viewer` output stays in the base module for both
flavors so saved models remain viewable after the removable creation runtime is
uninstalled. The large generation runtime remains outside the base module.

Build or install a locally deliverable Play debug bundle with:

```powershell
.\gradlew.bat :androidApp:buildPlayDebugLocalTestingApks -PphoneControlDeviceSerial=<serial> --console=plain
.\gradlew.bat :androidApp:installPlayDebugLocalTesting -PphoneControlDeviceSerial=<serial> --console=plain
```

The serial is mandatory for install and scopes the generated APK set. BundleTool
refreshes the exact device specification before each targeted build, so a reused
serial cannot silently reuse splits for a different ABI, density, locale, or SDK.
Omit the serial when building an APK set for all supported devices. If multiple
ADB installations exist, pass `-PlocalTestingAdbPath=<absolute-adb-path>` to select
the executable used for device inspection and installation. Use
`--no-configuration-cache` for these BundleTool tasks.

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
