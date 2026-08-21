# Release Checklist

Windows ships through GitHub Releases. Android Play flavor ships through Google Play. Keep machine secrets and private endpoints in gitignored `docs/RELEASING.local.md`.

## 1. Establish scope

```powershell
git status --short
git log <PREVIOUS_TAG>..HEAD --oneline
```

- Start from a clean, reviewed tree.
- Read every commit since the previous tag.
- Do not infer release notes from filenames alone.

## 2. Bump version

`[package].version` in `Cargo.toml` is the source of truth. Desktop and Android
derive their public version from it, but three surfaces repeat it and do not
follow automatically:

| Surface | Field |
| --- | --- |
| `app.rc` | `FILEVERSION`, `PRODUCTVERSION`, `FileVersion`, `ProductVersion` |
| `component-delivery/creation-runtime-v1.json` | `hostVersion` |
| `component-delivery/windows/external-tools-v1.json` | `hostVersion` |
| Host-bound Cargo packages such as `native/recorder_worker/Cargo.toml` | `[package].version` |

One command bumps all of them and reports what it changed:

```powershell
py -3 scripts/check_version_pins.py --write
```

Review that diff, then rerun without `--write` to confirm it exits clean.

Do not skip this in favour of building and seeing what breaks. Each `hostVersion`
is asserted by a *different* build script, so a build reports one stale pin and
halts; the next is only discovered after fixing the last. The Android build
asserts `creation-runtime-v1.json` too, so a pin missed here can fail after the
desktop build has already passed.

This is a script rather than a test for a concrete reason: the build scripts
panic during compilation, so `cargo test` cannot reach a test that would report
these — the build dies first. The script needs no compilation, and it finds the
manifests by scanning `component-delivery/`, so one added later is covered
without editing the table above.

`hostVersion` is only a host pin: bumping it does not invalidate the asset name,
`sha256`, or component `version` beside it, so a version bump never requires
republishing a runtime bundle.

## 3. Draft release notes

One release is one gitignored file: `tmp-release-notes-<VERSION>.md`. Every
channel lives in it as a `## ` section, so the whole release reads in one place.

```markdown
# v<VERSION>

## github
- plain English bullet
...

## play en-US
...

## play vi-VN
...

## announcement vi-VN
_Phiên bản tiếng Việt:_
...
```

Section names in use:

| Section | Goes to |
| --- | --- |
| `github` | GitHub release body. English bullets only, plus the two footer lines below. |
| `play <locale>` | Play listing for that locale, 500 characters max. |
| `announcement vi-VN` | Vietnamese write-up for the Zalo group. Never the GitHub body. |

`## ` at the start of a line delimits sections, so demote any heading inside a
section body. Read sections back with the helper rather than by hand:

```powershell
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --list
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --section github
```

The `github` section uses the established format: plain English bullets with no
added release heading, followed by the current Zalo support-group line, then a
horizontal rule and the VietQR donation footer:

```text
_Nhóm chat hỗ trợ SGT tại Việt Nam: https://zalo.me/g/arxevk379_

---

💙 **Ủng hộ tác giả** — Người dùng Việt Nam có thể ủng hộ qua VietQR: [bấm vào đây](https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT).
```

Those two trailing lines are the only non-English copy allowed. Keep the
donation link identical to the one in `README.md`. Never include Vietnamese
release notes, Google Play notes, or store metadata in a GitHub release body.

Map every bullet to a real commit. Owner reviews notes before any publish step.

Each `play <locale>` section has a 500-character limit. Never publish a `play`
section as the GitHub body or the reverse: the GitHub body carries the Zalo and
VietQR lines, which do not belong on a store listing.

## 4. Refresh help index

Start the private embedding service described in `docs/RELEASING.local.md`, then:

```powershell
python scripts/help_index_build.py
git diff --stat -- help-index.json
```

Confirm `help-index.json` changed for the intended source tree and contains no local secrets.

## 5. Validate

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
.\scripts\validate-windows-targets.ps1
```

Run relevant frontend/mobile tests for changed subsystems. Do not waive failures to cut a release.

### Promote optional-component candidates

Development candidates live on `sgt-runtime-staging`; a release host may not
reference that tag. For every changed delivery contract, promote the already
tested candidate before the component-specific checkpoint below:

```powershell
py -3 .\scripts\component_release.py verify-staging
py -3 .\scripts\component_release.py promote `
  --contract-relative component-delivery/windows/recorder-v1.json `
  --output $env:TEMP\recorder-v1.promoted.json
git diff --no-index -- `
  .\component-delivery\windows\recorder-v1.json `
  $env:TEMP\recorder-v1.promoted.json
```

Review the promoted manifest, then either update the tracked contract
deliberately or rerun promotion with
`--apply-tracked .\component-delivery\windows\recorder-v1.json`. Promotion:

- downloads and hashes the staging bytes;
- reuses an identical production asset or uploads a new one without overwrite;
- downloads and hashes the production bytes again; and
- rewrites only staging URLs to `sgt-runtime-bundles`.

Use `--clean-staging` only after the tracked production contract and dependent
platform manifests are reviewed. It removes the promoted candidate from the
mutable staging release but never removes production bytes. If no remote test
was needed, upload the deterministic content-addressed package directly through
the existing component checkpoint; do not create a fake staging record.

Canonical package workspaces are under
`%LOCALAPPDATA%/SGT-Development/cache/packages/release`; `build.ps1` selects that
root automatically (or `SGT_DEV_CACHE_ROOT`) and keeps worker Cargo artifacts in
the separate `cargo/package` lane. The old ignored `local-runtime-bundles`
model/ASR manifests are accepted only as a migration fallback. Move newly
generated packages to the managed cache and prune the legacy directory after
its active release checkpoint is complete.

The component-specific commands below use this shared setup:

```powershell
$env:SGT_DEV_CACHE_ROOT = Join-Path $env:LOCALAPPDATA "SGT-Development\cache"
$componentPackageRoot = Join-Path $env:SGT_DEV_CACHE_ROOT "packages\release"
$packageCargoTarget = Join-Path $env:SGT_DEV_CACHE_ROOT "cargo\package"
New-Item -ItemType Directory -Path $componentPackageRoot -Force | Out-Null
```

For a release containing Image-to-3D, Image-to-SVG, or image creation/editing,
build the separately tracked runtime first and update the tracked delivery
contract only after remote read-back. Each capability is active on Windows,
Android Full, and Android Play only when its release-availability fixture
enables it. A capability can remain checksum-pinned and packaged while its
launcher and job admission are temporarily disabled.

### Mandatory creation-runtime release checkpoint

This is a blocking release requirement. Do not proceed to the Windows or
Android host builds until every item is complete:

1. Rebuild the Windows creation runtime when its packaged bytes can change.
2. Rebuild both Android creation-runtime distributions when their packaged
   bytes can change. Rebuild only the affected deterministic platform package.
3. Regenerate the affected platform manifest and a combined delivery candidate.
   The candidate must advertise exactly `image_to_3d`, `image_to_svg`, and
   `image_creator`; its `hostVersion` must equal the root `Cargo.toml` package
   version. Preserve the latest verified identity for an unaffected platform.
4. Upload every rebuilt runtime under a new, uniquely versioned asset name on
   the existing GitHub runtime-bundles release. Never replace or delete an asset
   referenced by a released host; older signed hosts must keep resolving the
   exact bytes they were built to verify. A local-only build is not a completed
   runtime release for any platform whose bytes changed.
5. Query the GitHub release again and verify every uploaded asset's name, size,
   and checksum against the newly generated manifests.
6. Commit that exact verified combined delivery contract at
   `component-delivery/creation-runtime-v1.json` for all subsequent
   Windows, Android Full, and Android Play app builds.

The private runtime source and its build scripts live in the nested, gitignored
`native/sgt_3d_generator_runtime/` repository; its build output lands in the
gitignored `local-runtime-bundles/sgt_creation_runtime/`.

```powershell
.\native\sgt_3d_generator_runtime\scripts\build_exe.ps1
.\native\sgt_3d_generator_runtime\scripts\build_android_runtime.ps1 `
  -CopyToBundleDirectory -UpdateHostPins -Publish
py -3 .\scripts\verify_creation_runtime_release.py --manifest .\component-delivery\creation-runtime-v1.json
```

Native libraries must be built for 16 KB memory pages or Play rejects the
update. Verify before publishing that every `PT_LOAD` segment in every `.so`
inside the AAB reports `p_align` of at least 16384.

The same manifest version and feature handshake must feed Windows, Android Full,
and Android Play. The tracked contract is build input, so ordinary and canonical
builds resolve identical immutable locations and integrity metadata. Missing or
invalid tracked delivery data is a compile-time failure, never a local fallback.

### Mandatory Android native-ASR runtime checkpoint

Android Full downloads ORT, Moonshine, and Sherpa on first use; Android Play
delivers the same reviewed bytes in on-demand feature modules. Their shared
authority is `parity-fixtures/phone-control/native-runtime-contract.json`.

When any native runtime changes:

1. Rebuild it from its pinned source contract. A reduced ORT build must also run
   `mobile/scripts/smoke-ort-runtime.ps1` on a physical arm64 device and produce
   the expected representative transcript before its archive can be adopted.
2. Package deterministically, assign a new content-addressed runtime-bundles
   asset name, and update the exact archive/member sizes and SHA-256 values plus
   `downloadUrl`. Never overwrite the legacy fixed-name assets.
3. Upload the new asset without `--clobber`, then verify the checked-in archives
   and all remote identities:

```powershell
cd mobile
.\gradlew.bat verifyNativeRuntimeArchives --console=plain
cd ..
py -3 .\scripts\verify_update_catalog_sources.py
```

Do not accept a successful native link as runtime evidence. The physical ORT
smoke, ELF64/AArch64 export and dependency checks, 16 KB `PT_LOAD` alignment,
deterministic archive identity, and remote read-back must all pass.

Whenever mini-app, worker, runtime, or packaged frontend source changes, rerun
that component's checkpoint below even if the host API did not change. Upload
only when deterministic packaging produces a new digest; use a new
content-addressed name, read it back, and retain every older published asset.
Do not release a host whose tracked contract still describes the pre-change
bytes.

The `sgt-runtime-bundles` tag is an append-only artifact store. Every executable,
native library, model, or WebView pack uses a unique asset name and an exact
size/SHA-256 recorded in the signed host build. HTTPS is transport, not trust:
hosts reject bytes that do not match the pinned delivery record. Release
automation must fail if an upload would overwrite an existing asset.

### Mandatory Windows mini-app web-pack checkpoint

The 3D Creation, PromptDJ, and TTS Playground interfaces ship as optional,
removable Windows components rather than bytes embedded in the desktop host.
Complete this checkpoint before `build.ps1`:

1. Build the three reviewed frontends and create deterministic ZIP packages.
2. Upload every newly generated, content-addressed ZIP to the existing
   `sgt-runtime-bundles` release. Never replace or delete an older pack.
3. Read the release back and generate delivery metadata only after the remote
   names, sizes, and SHA-256 values match the local packages.
4. Keep the verified delivery manifest at the path below. `build.ps1` rebuilds
   the packs and stops if the frontend bytes differ from that manifest.

```powershell
.\scripts\build-web-asset-packs.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_web_assets")
# Upload only the new ZIP files listed in the generated packages manifest.
py -3 .\scripts\verify_web_asset_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_web_assets\sgt_web_assets.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_web_assets\sgt_web_assets.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_web_assets\sgt_web_assets.delivery.json") `
  .\component-delivery\windows\web-assets-v1.json
```

The verifier is read-only: it downloads the published assets and hashes their
actual bytes before emitting delivery data. Every build consumes the tracked
contract; absent or divergent verified delivery is a build failure.

### Mandatory Windows external-tool checkpoint

yt-dlp, FFmpeg/ffprobe, and Deno are independently removable x64 components.
The WebView2 Evergreen bootstrapper is also pinned here, but the Microsoft-managed
WebView2 Runtime it installs is shared system software and is never removed by SGT.
Reproduce the reviewed packages, upload only the new content-addressed FFmpeg and
WebView2 assets, then read every remote byte back before building the host:

```powershell
.\scripts\build-external-tool-packs.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_external_tools")
# Upload only the new SGT assets listed by sgt_external_tools.packages.json.
# yt-dlp and Deno remain on their immutable upstream version tags.
py -3 .\scripts\verify_external_tool_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_external_tools\sgt_external_tools.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_external_tools\sgt_external_tools.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_external_tools\sgt_external_tools.delivery.json") `
  .\component-delivery\windows\external-tools-v1.json
```

The verifier checks exact archive and installed-file inventories. It additionally
requires the reviewed WebView2 file version, Microsoft publisher, and valid
Authenticode signature. `build.ps1` stops before compiling when the read-back
manifest is absent, targets another host version, or differs from the reviewed
local artifacts.

Microsoft's [Evergreen distribution guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version)
explicitly permits downloading the Evergreen bootstrapper and packaging it with
an app. SGT uses that redistribution option for the exact verified bootstrapper
copy only. The bootstrapper installs Microsoft's shared, automatically serviced
Runtime; SGT does not package or uninstall that system Runtime.

### Mandatory Windows model-delivery checkpoint

Windows model payloads are independent removable components. Use
`scripts/package_windows_models.py` with the reviewed source directory to
produce the canonical ignored package directory, regenerate the tracked
delivery file deterministically, then verify both the local inventories and
their immutable remote objects:

```powershell
py -3 .\scripts\generate_windows_model_delivery.py `
  --package-manifest (Join-Path $componentPackageRoot "sgt_windows_models\sgt_windows_model_packages.json") `
  --output .\model-delivery\windows-v1.json
py -3 .\scripts\verify_windows_model_release.py `
  --package-manifest (Join-Path $componentPackageRoot "sgt_windows_models\sgt_windows_model_packages.json") `
  --delivery-manifest .\model-delivery\windows-v1.json --remote
```

The canonical build repeats deterministic generation comparison and hashes
every local archive and installed-file entry before compilation. Remote
verification is a separate mandatory release checkpoint because some immutable
model objects are too large to download again during every host build.

### Mandatory Windows VC support checkpoint

The shared x64 VC support package is independently versioned and removable.
Prepare and verify it with the same append-only release rule before a Windows
release build:

```powershell
.\scripts\build-vc-runtime-pack.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_vc_runtime")
# Upload only the new ZIP named by sgt_vc_runtime.packages.json.
py -3 .\scripts\verify_vc_runtime_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_vc_runtime\sgt_vc_runtime.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_vc_runtime\sgt_vc_runtime.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_vc_runtime\sgt_vc_runtime.delivery.json") `
  .\component-delivery\windows\vc-runtime-v1.json
```

The pack contains an exact per-file size/SHA-256 contract as well as the
archive size/SHA-256. `build.ps1` rejects a local pack that differs from the
read-back-verified delivery record. The statically linked host embeds no VC
payload and does not download one during startup. Each optional native feature
ensures this component at first use and retains its lease for the full native or
worker lifetime.

### Mandatory Qwen3 CUDA runtime checkpoint

Qwen3 uses one small runtime/notices pack and two content-addressed libtorch
packs, each below GitHub's per-asset limit. The selected runtime inventory and
all required notices are exact files in the delivery manifest.

```powershell
.\scripts\build-qwen3-runtime-pack.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_qwen3_runtime")
# Upload only the three new ZIPs listed in sgt_qwen3_runtime.packages.json.
py -3 .\scripts\verify_qwen3_runtime_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_qwen3_runtime\sgt_qwen3_runtime.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_qwen3_runtime\sgt_qwen3_runtime.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_qwen3_runtime\sgt_qwen3_runtime.delivery.json") `
  .\component-delivery\windows\qwen-runtime-v1.json
```

The verifier hashes the uploaded bytes before emitting host delivery data.
Never replace either split libtorch pack, even when a newer host stops using it.

### Mandatory local ASR checkpoint

Local ASR ships a standalone x64 worker and a separate ONNX/DirectML runtime;
the latter depends on the VC component. Build and package them explicitly, then
upload only their new content-addressed ZIPs and read them back before building
the host:

```powershell
.\scripts\build-local-asr-packs.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_local_asr") `
  -CargoTargetDir $packageCargoTarget
# Upload only the two new ZIPs listed in sgt_local_asr.packages.json.
py -3 .\scripts\verify_local_asr_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_local_asr\sgt_local_asr.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_local_asr\sgt_local_asr.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_local_asr\sgt_local_asr.delivery.json") `
  .\component-delivery\windows\local-asr-v1.json
```

`build.ps1` never builds or embeds these native packages. It requires the
read-back-verified manifest and fails before compiling the host when it is
absent.

### Mandatory Screen Recorder checkpoint

Screen Recorder ships as two independently removable x64 components: the
standalone native worker and its frontend. Reproduce the content-addressed
packages, upload only those new immutable ZIPs, then verify the remote bytes:

```powershell
.\scripts\build-recorder-component-packs.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_recorder") `
  -CargoTargetDir $packageCargoTarget
# Upload only the two new ZIPs listed in sgt_recorder.packages.json.
py -3 .\scripts\verify_recorder_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_recorder\sgt_recorder.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_recorder\sgt_recorder.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_recorder\sgt_recorder.delivery.json") `
  .\component-delivery\windows\recorder-v1.json
```

The worker package includes its exact third-party license inventory. The host
build fails closed when either remote package is missing or differs from the
read-back-verified size, SHA-256, or file inventory. Never replace an existing
runtime-bundles asset.

### Mandatory Computer Control engine checkpoint

Computer Control keeps credentials, provider transport, audio, observations,
confirmations, effects, receipts, and process supervision in the signed host.
Its data-only planning and provider-protocol engine is a separate removable x64
component. Reproduce it, upload only its new content-addressed ZIP, then verify
the published bytes:

```powershell
.\scripts\build-computer-control-engine-pack.ps1 `
  -OutputDir (Join-Path $componentPackageRoot "sgt_computer_control") `
  -CargoTargetDir $packageCargoTarget
# Upload only the new ZIP named by sgt_computer_control.packages.json.
py -3 .\scripts\verify_computer_control_release.py `
  --packages (Join-Path $componentPackageRoot "sgt_computer_control\sgt_computer_control.packages.json") `
  --output (Join-Path $componentPackageRoot "sgt_computer_control\sgt_computer_control.delivery.json")
py -3 .\scripts\verify_tracked_delivery.py `
  (Join-Path $componentPackageRoot "sgt_computer_control\sgt_computer_control.delivery.json") `
  .\component-delivery\windows\computer-control-v1.json
```

The component contains the x64 engine plus its complete resolved third-party
license inventory and notices. `build.ps1` rebuilds the engine and fails closed
unless the result exactly matches the asset that the verifier downloaded from
the append-only runtime-bundles release.

### Mandatory host-carried notice checkpoint

The compact language catalog remains host-carried data even though the upstream
runtime crate is no longer linked. Keep
`native/language_catalog/THIRD-PARTY-NOTICES.txt` with the release source and
verify its isolang 2.4.0 Apache-2.0 attribution before signing. Do not restore
the upstream language-table payload; the derived catalog parity tests must
continue to cover all mappings.

### Mandatory signed component-catalog checkpoint

The host and Android Full discover newer optional-component contracts through a
detached ECDSA P-256 signed catalog. The catalog changes delivery metadata only;
it never weakens the exact size/SHA-256 checks in each installer. Android Play
native code remains owned by Play dynamic-feature delivery and must not be
replaced by a GitHub-downloaded executable. The Play base performs no GitHub
catalog discovery: its immutable model contracts advance with the next reviewed
Play build. Android Full may select newer signed model-data transport.

After every referenced component checkpoint above is green:

1. Increment `sequence` in
   `component-delivery/update-catalog-v1.sources.json`; a published sequence is
   immutable and is never reused for different source bytes.
2. Set host compatibility bounds deliberately. A host outside the range rejects
   the catalog and continues using its compiled delivery contracts.
3. Run `py -3 scripts/verify_update_catalog_sources.py`. It must read back every
   SGT-owned `sgt-runtime-bundles` asset and match its recorded size and GitHub
   SHA-256 digest.
4. Commit and push the reviewed sources, then run the
   `Publish component catalog` workflow. The signing key exists only as the
   repository secret `SGT_UPDATE_CATALOG_P256_PRIVATE_KEY_PEM_BASE64`.
5. Confirm the workflow read-back gate. It uploads only missing
   content-addressed catalog/signature assets and refuses any same-name byte
   mismatch. Its deterministic signature makes a partial workflow retry
   byte-identical.

Never delete or replace an older runtime, model, catalog, or signature asset.
Old signed app versions may still reference it. Updating yt-dlp or Deno is
failure-triggered and retries one request; Creation retries only before a job is
accepted; FFmpeg updates while idle only for users who already installed an
older managed FFmpeg. Other optional components select the newest compatible
contract immediately before their next open/session/install boundary.

## 6. Build Windows

```powershell
.\build.ps1
```

Expected build artifacts:

- `target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe`

Smoke-test the x64 artifact on suitable hardware. GitHub publishes the x64 artifact only.

## 7. Build Android

The release wrapper always builds the signed full-flavor APK. `-IncludeAab` also builds the Play AAB:

```powershell
.\mobile\build-release.ps1 -IncludeAab
```

Expected copied artifacts:

- `target/release/ScreenGoatedToolbox_v<VERSION>.apk`
- `target/release/ScreenGoatedToolbox_v<VERSION>.aab`

The Play AAB is the store artifact. Treat the full APK as development/direct-distribution output only.

## 8. Finalize the release commit and tag

After owner review, commit the release changes, then create and push the tag from that exact commit:

```powershell
git status --short
git tag -a v<VERSION> -m "Screen Goated Toolbox v<VERSION>"
git push origin HEAD
git push origin v<VERSION>
```

Verify the remote tag resolves to the reviewed release commit before creating a release.

## 9. Draft GitHub release

Create a draft first; use paths produced by step 6:

```powershell
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --section github |
  Set-Content -Path tmp-github-body.txt -Encoding utf8
gh release create v<VERSION> `
  --verify-tag `
  --draft `
  --title "Screen Goated Toolbox v<VERSION>" `
  --notes-file "tmp-github-body.txt" `
  "target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe" `
  "target/release/ScreenGoatedToolbox_v<VERSION>.apk"
```

Every GitHub release must attach both the Windows x64 executable and the Full
Android APK. Review title, body (English bullets plus the step 3 Zalo and
VietQR footer), binaries, sizes, and checksums in GitHub UI. Publish only after
owner approval.

## 10. Publish Google Play

Explicit owner approval after review is the publication gate for both release
channels. Once approval is given, publish the reviewed GitHub draft and submit
the reviewed AAB to the Google Play production track without requesting a second
confirmation. Until then, keep the GitHub release as a draft and do not commit a
Play release edit.

Upload the AAB in Play Console, or use the repository helper:

```powershell
python -m pip install google-api-python-client google-auth
$env:PLAY_SERVICE_ACCOUNT_JSON = '<path to the Play service-account JSON>'
python scripts/play_publish.py `
  --aab "target/release/ScreenGoatedToolbox_v<VERSION>.aab" `
  --track production `
  --notes-md "tmp-release-notes-<VERSION>.md" `
  --fraction 1.0
```

Publish straight to `production`. Never use the `internal` track. `--notes-md`
publishes every `## play <locale>` section in one release, so locales cannot
drift apart. Keep the service-account JSON outside the repository.

### Send the release for review (Play Console, manual)

The helper reports which commit path it took:

- `Google review follows for production.` — the edit was submitted, nothing else
  to do here.
- `NOT yet submitted: ...` — the API refused to submit and committed with
  `changesNotSentForReview=true`. The release exists on the track but no review
  has started, so it will never reach users until it is sent by hand.

Google exposes no API call to submit an already-committed edit; it is Console
only. After a rejection Play forces this path for every later edit.

1. Play Console → the app → **Publishing overview**.
2. Read the banner first. **Some recent changes were rejected** means a previous
   submission failed review; open **Policy status** and confirm every listed
   violation is actually fixed before resubmitting.
3. Check **Policy status → Policy issues** for anything under *App updates with
   these issues will be rejected*. Those block approval even when the release
   itself is fine.
4. Only once those are clear, use **Submit N changes for review** on Publishing
   overview.
5. Confirm the release moves to *In review* on **Test and release → Latest
   releases and bundles**.

Do not resubmit while an appeal is still open on the same violation; wait for
the appeal outcome so the appeal and the new submission do not conflict.

## 11. Finish

- Publish GitHub draft.
- Confirm the Play release reached *In review*, then *Available on Google Play*.
  A committed release that was never sent for review reaches nobody.
- Verify download/install/update paths from a clean client.
- Record any release-only caveat in durable docs, not temporary chat notes.
