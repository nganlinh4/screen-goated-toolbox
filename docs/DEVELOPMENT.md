# Development

Use this document for commands and repository structure. AI-specific invariants live in [`../AGENTS.md`](../AGENTS.md); subsystem contracts live beside their code.

## Desktop setup

Required on Windows:

- Current stable Rust + Cargo.
- Visual Studio 2022 Build Tools with Desktop development with C++.
- Node.js + npm for embedded frontends.
- Microsoft Edge WebView2 Runtime.

Fresh clones must build frontends before Rust. The PromptDJ, Translation Gummy and
Screen Record `dist/` outputs are not tracked. Three `dist/` trees are tracked
(`.gitignore` re-includes them) and have separate delivery owners: Image to SVG is
embedded in the host, TTS Playground is packaged by `scripts/package_web_assets.py`,
and 3D Generator is packaged into the creation component by
`scripts/package_creation_windows.py`. `run-dev.ps1` rebuilds all six and fails
when a build does not refresh its `dist/`:

```powershell
.\run-dev.ps1
```

Tray and window PNGs under `assets/` are source inputs. Cargo reads these checked-in
images without rewriting them;generated Windows icon resources belong in `OUT_DIR`.

Useful options:

```powershell
.\run-dev.ps1 -SkipFrontendBuild
.\run-dev.ps1 -SkipNpmInstall
.\run-dev.ps1 -AuditFrontendDependencies
.\run-dev.ps1 -CargoCommand test
```

The normal launch still reconciles every frontend dependency, runs install
scripts, and rebuilds all embedded frontends. It disables npm's remote advisory
request because registry latency must not block the development loop. Use
`-AuditFrontendDependencies` when an explicit dependency-security audit is the
task; CI retains its independent clean-install checks.

`run-dev.ps1` uses `%LOCALAPPDATA%/SGT-Development/cache/cargo/dev` instead of a
repository `target/` tree. Logs use the same external cache under
`evidence/dev-run-logs`. The separate package-build lane is `cargo/package`, so
routine host iteration does not invalidate expensive worker/package artifacts.

Build output lives outside the checkout on purpose: the checkout holds only
source, package-job worktrees reuse the one package lane instead of each
growing a `target/` of their own, and everything generated sits in one bounded,
disposable root. Nothing in
the cache is ever a delivery source; deleting all of it costs only rebuild time.
Machine-local state that is not a cache stays beside it, not inside it:
`sgtp.json` (the phone endpoint) and `manual-tests/` (screen-translate lab
inputs).

Every `run-dev.ps1` launch prunes the cache with the dev lane protected:

- Always: linked executables superseded by two newer ones of the same crate
  (with their program databases), and codegen objects an interrupted build left
  behind for over a day.
- After 14 inactive days: evidence, candidate packages, legacy lanes and
  unprotected Cargo lanes, and each isolated runtime state root under
  `runtime/` (the `SGT_RUNTIME_STATE_ROOT` of a test run).
- Above 28 GiB, oldest first: the same entries except runtime state roots, then
  incremental compiler state, so the next build of that crate starts cold.

The pruner keeps anything a running process names or holds open, and never
touches a source checkout, user output, staging contract, promotion record, or
an installed app's runtime components.

```powershell
.\scripts\dev-cache.ps1 -Action Status
.\scripts\dev-cache.ps1 -Action Prune                  # dry run
.\scripts\dev-cache.ps1 -Action Prune -Apply
```

Set `SGT_DEV_CACHE_ROOT` to relocate the cache. Use `-DevCacheLimitGiB` on
`run-dev.ps1` only when the machine deliberately needs a different bound.

`libs/egui-snarl`, `libs/egui-scale`, and `libs/epaint` are disposable dependency
trees reconstructed from pinned revisions or crates.io bytes and tracked patches.
The epaint patch removes Vello's unreachable quality/f32 glyph pipeline; SGT's
egui font rasterizer uses the speed/u8 pipeline exclusively. SGT-owned egui
shaders are validated and translated at build time; WGPU retains its internal
WGSL support because indirect draw and dispatch validation require it.
Never make a product change directly in those ignored directories. Edit the
tracked `.patch` file under `scripts/`, then run:

```powershell
.\scripts\setup-egui-snarl.ps1
.\scripts\validate-egui-patches.ps1
```

The development and release wrappers enforce an exact patched-tree match; a
marker string or matching Cargo dependency line is not sufficient.

`-BuildLocalCreationRuntime` remains a source diagnostic for maintainers of the
separate checkout; the app never loads that local executable. Normal debug and
release behavior uses only an exact external delivery contract.

## Restart handoff

Application restart and update handoff uses a direct native executable path.
The replacement consumes the existing parent identity before crash-monitor or
singleton startup and waits for that exact process to exit. Future update builds
must retain this startup contract. Ordinary restart retains only `--restarted`;
update and WebView recovery do not replay file activations or diagnostic flags.
Update launch holds the verified canonical file and directory guards through
process creation. A failed spawn leaves exit preparation available for retry;
successful process creation still requires normal startup acceptance.

## Windows startup registration

Ordinary startup owns the application's exact current-user Run value and its
StartupApproved value. It does not change machine-wide startup entries. Known
failed provider switches restore the prior raw values and refuse to overwrite
observed concurrent changes. A failed registration RPC remains uncertain even
after an instantaneous read-back; that operation keeps ordinary startup suspended.
There is no cross-service transaction or inferred server-quiescence guarantee.
New commands quote the native executable path.
Ordinary startup accepts absolute UNC paths without probing remote files.
Known legacy UNC task targets can be retired by identity; elevated execution
continues to require a verified fixed local volume. Automatic ownership takeover
requires confirmed absence through a guarded local path; remote, reparse and
inaccessible references are preserved for explicit user selection.
New Run commands and task paths use a conservative limit of 260 UTF-16 units (excluding
the Run value's terminating NUL), validated before mutation. Legacy raw values
remain available for exact retirement and rollback; oversized Run entries are
not reported as enabled.

Elevated startup requires a fixed local volume with persistent ACLs, trusted
file/directory ownership and protection against unprivileged replacement. The
executable and ancestor guards remain held through registration and read-back.
Reparse paths, ambiguous aliases and unsupported security descriptors are refused.
Task permissions must preserve administrator control and give the user only
read/execute access. A writable portable location cannot be used for elevated
startup; install the application with administrator-protected permissions first.

Native task inspection checks the exact action, arguments, logon trigger and
current-user principal. Identified unsafe legacy registrations are disabled;
unknown registrations and access failures are kept distinct from missing tasks.
Stored startup paths authorize legacy retirement only, never elevated execution.
Startup preferences advance only after the selected provider transition verifies.
Isolated development state never changes system startup registration.

Settings initialization, settings changes and the startup-disable step of App settings
reset use a serialized background worker. Startup controls retain confirmed
preferences while an operation is pending. Rendering polls owned completions;
only matching startup preferences can accept a result, and unrelated edits are
preserved through the normal configuration merge. Failed or uncertain changes
do not commit the requested mode. Reset continues only after verified disable.
A process-wide permit defers quit/restart through native work and completion
persistence; hiding the settings window does not cancel an operation. Native RPC
calls are not assumed cancelled or quiescent after a timeout or uncertain result.

The explicit `--sgt-autostart` launch retains the 20-second stabilization delay.
Legacy inference applies only to launches without arguments while uptime is below
180 seconds. Unknown or later uptime skips registration inspection; unrelated
arguments skip both inference and the delay. `cargo test startup_launch::tests`
checks these boundaries with an injected registration predicate.

`cargo test admin_startup` exercises injected scheduler/registry implementations,
native security descriptors and owned filesystem fixtures. It does not connect to
Task Scheduler or change the user's startup registry values. Actual logon and
elevated-startup acceptance remains a separate application acceptance step.

## Download Manager initialization

Browser discovery starts on the first visible Download Manager render, in one
read-only background worker. Rendering polls its completion without waiting;
closing and reopening the window reuses the same scan. Discovery updates only the
available cookie-browser choices, preserving the configured selection. Results
are cached for that manager instance; failures are logged once without retrying
on every frame. `cargo test browser_discovery::tests` exercises deferred execution,
pending/completed reuse, failure and owner destruction with injected detectors.

## Optional-component candidate loop

Production components are immutable, but development candidates do not belong
on the append-only production release. Use the mutable prerelease
`sgt-runtime-staging`, which stores at most the current candidate for each
delivery contract. The package wrapper writes all archives, Cargo artifacts,
and evidence into the bounded external cache:

```powershell
.\scripts\build-component-candidate.ps1 -Component recorder -Stage
.\run-dev.ps1 -UseStagingDelivery
```

For Creation frontend work, the second command is sufficient: after building
the frontend, `run-dev.ps1` compares every packaged web file with the local
staging contract and rebuilds/stages Creation only when the bytes differ.

Supported wrapper names are `creation`, `web-assets`, `recorder`, `computer-control`,
`local-asr` (the ONNX/DirectML runtime), `vc-runtime`, and `external-tools`. `-Select <id>`
may update one component in a multi-component contract; repeat it when several
components changed. Android native runtimes and model packages use their
subsystem packager, then call `scripts/component_release.py stage` with the
generated package manifest and tracked contract.

Staging is a real network install: the script verifies local bytes, uploads a
content-addressed candidate, downloads it again, and writes its exact contract
under `%LOCALAPPDATA%/SGT-Development/cache/staging/contracts`. A staging debug
build has its own `runtime/staging` registry root, installs on first use, and
then reuses that verified install across ordinary rebuilds. It disables the
production update catalog for the session. It does not read a package from the
checkout or fall back to a local worker.

`-UseStagingDelivery` is compile-time restricted to the debug profile.
`build.ps1` rejects staging environment variables before doing work, and the
Rust build script independently rejects staging for any non-debug profile.
Close the staging debug app before pruning its runtime cache.

When `SGT_RUNTIME_STATE_ROOT` is set to an absolute path, the main configuration
is read and saved under that root. A new root starts with default configuration;
it does not import the user's existing settings or API keys. Provider credentials
in inherited environment variables are ignored; configure synthetic or test
credentials explicitly in the isolated settings when needed. At process entry,
credential-named inherited variables are removed before child processes start;
environment variables with unrelated names are not inspected for secret values.
Isolated startup
does not preconnect the speech service, even when credentials are configured;
an explicit speech request initiates that connection. The recorder worker
inherits the same root and reads that isolated configuration. User-selected
exports and storage managed by external tools remain outside this boundary.

## Rust validation

Shortcut naming, capture adapters, and audited consumers are documented in
[`src/hotkey/README.md`](../src/hotkey/README.md).

During implementation, run the narrowest relevant test and a fast host check:

```powershell
cargo test <module-or-test-name>
cargo check --bin screen-goated-toolbox
```

At a repository checkpoint, run the complete non-release gates once:

```powershell
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

The root package does not include native worker/protocol packages. Run their
[shared checkpoint](../native/README.md) with `python -m scripts.check_native`.

## Automated development checks

The [Development workflow](../.github/workflows/development.yml) runs root Windows
Rust and native-package checks, JavaScript unit and browser contracts, Python tooling tests, mini-app
tests/builds and both Android JVM flavors. Its `Development gate` succeeds only
when every selected job succeeds. The scope job can skip checks for changes only
to the root README/agent guidance or this document and the release guide; it still
reports the gate. Repository branch protection should require `Development gate`.
These jobs do not publish components or run release wrappers.

Install the locked recorder dependencies for shared DOM/browser test fixtures.
The text-input unit tests use Python 3 to generate the Android overlay assets.

```powershell
npm --prefix screen-record ci
node scripts/test-javascript.mjs --suite unit
node scripts/test-javascript.mjs --list
```

The runner uses Node's [test CLI](https://nodejs.org/docs/latest-v24.x/api/test.html)
and discovers `*.test.js`, `*.test.cjs`, `*.test.mjs` and corresponding
TypeScript files below `src/`, `scripts/tests/` and `tests/`. Browser-backed suites
must use `*.browser.test.*`; the unit profile runs Node/VM/JSDOM fixtures without
launching Chromium. The default profile runs both kinds. To run browser contracts:

```powershell
Push-Location screen-record
npx playwright install chromium
Pop-Location
node scripts/test-javascript.mjs --suite browser
```

These isolated browser fixtures supplement the native WebView2 acceptance checks
below. They do not exercise the user's applications or browser profile.

Run all Python tooling tests from the repository root:

```powershell
python -m pip install -r scripts/test-requirements.txt
python -m unittest discover -s scripts/tests
```

Mini-app unit scripts stay in their owning `package.json`; CI reports explicitly
when a package has no unit script and still runs its typecheck/build. Interactive
desktop/provider Rust tests remain explicit opt-ins through `#[ignore]`, rather
than silently succeeding when their external prerequisite is absent.

The [Rust Advisories workflow](../.github/workflows/rust-advisories.yml) scans
the host and every repository-owned native lockfile on dependency changes and
weekly against the [RustSec database](https://rustsec.org/). To run it locally,
install the pinned [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
version and use:

```powershell
cargo install cargo-audit --version 0.22.2 --locked
python -m scripts.check_rust_advisories --list
python -m scripts.check_rust_advisories
```

The scanner fetches the database once and checks every lockfile against that
snapshot. Vulnerabilities and incomplete/error results fail the check; known
vulnerabilities are not suppressed. Yanked-version queries are disabled to keep
this check focused on RustSec advisories. An optional `--report <file.json>`
retains the full per-lockfile reports, including informational warnings.

## User diagnostic log

Speech capture uses `src/api/audio/pcm_capture.rs` for common integer/float
formats and a callback-partition-independent PCM converter. It applies no gain.
Its activity summaries report frame counts and peak frame RMS, in addition to
cumulative capture levels. Quiet speech uses the noise-relative activity contract
in `parity-fixtures/preset-system/microphone-activity.json`; local activity is not
proof of linguistic speech. Recording auto-stop separately retains its fixed
RMS > 0.015 trigger, 800 ms silence cutoff, and 2-second minimum speech window.
Quieter audio is captured unchanged but does not arm/reset auto-stop.
Continuous preset audio attachments are complete or
absent: after ten minutes they are omitted, while transcription continues.
Pending capture/reconnect audio is bounded to sixty seconds; overflow is logged.

All Windows microphone capture uses `src/audio_input.rs`, compiled by both the
host and recorder worker: audio presets, realtime transcription/translation,
Computer Control, Translation Gummy, TTS Playground source/reference recording,
and Screen Recorder microphone tracks. It prefers the default communications
input endpoint, falling back to the general default input only when the
communications endpoint cannot be resolved. Device names, virtual-device status,
mute, and silence do not override the user's role choices. Selection is resolved
afresh when capture starts. Output loopback and Windows defaults are unchanged.

The desktop app keeps routine diagnostic output in the single file
`%LOCALAPPDATA%\SGT\logs\session.log`. When the file would exceed 16 MiB, SGT
atomically compacts it to its newest 12 MiB at a complete-line boundary. The
retention window is therefore activity-based rather than a fixed number of
days. For routine support, request `session.log`.

Process diagnostics live under `%LOCALAPPDATA%\SGT\crashes`. Each session has
an executable SHA-256 and load address, version, architecture, profile, process
role, PID, and creation time in `identity.json`. Rust panic summaries and forced stack traces
are written synchronously; the first origin is preserved and the other three
report slots rotate. Panic reporting does not show a modal dialog. Renderer
children use the same reporting without diagnostic output on their protocol stdout.

A separate diagnostic observer captures native exception minidumps and records
the actual Windows exit status in `exit.txt`. `exit-intent.txt` identifies a
requested exit; missing intent means an unexplained exit, not proof of a crash.
The host also records renderer generation, PID, and exit status in its synchronous
`events.log`, bounded to the newest 64 KiB at complete-line boundaries.
The observer runs from the same executable, needs no administrator privileges,
and does not change Windows Error Reporting settings. Panic and native snapshots
are limited to one each per process; dumps larger than 32 MiB are discarded while
text evidence is retained. Native snapshots are best effort: fail-fast,
forced termination, or exhausted resources may leave only the observed exit
status and earlier text evidence. Power loss can prevent even an exit record.

Retention preserves live and uncertain sessions and unfamiliar files. Verified
inactive sessions keep up to eight incident sessions and eight routine sessions
within a shared 128 MiB budget, prioritizing incident evidence; active sessions
can temporarily exceed that budget. Existing legacy crash dumps are preserved.
`SGT_RUNTIME_STATE_ROOT` isolates these reports along with the runtime logs.
For crash investigation, collect the affected session directory and `session.log`.
Reports and dumps stay local; dumps can contain application memory. Resolving
stripped release addresses also requires the matching executable/PDB from that
exact build; a same-version development PDB is not interchangeable.

Normal logging retains startup, session/backend transitions, completion summaries,
retries, errors, and rendering failures. Repeated compositor acknowledgements,
successful card/fit milestones, intermediate overlay timings, and individual
transcription/paste updates require `SGT_VERBOSE_LOGS=1` at launch. This switch
also restores detailed successful vision-call metrics and five-second capture
health summaries. It does not enable transcript contents. Unit tests do not
append to the user's session log or Computer Control JSONL logs.

Computer Control's `SGT/logs/cc-events.jsonl` is a rolling global diagnostic log
limited to 16 MiB. Compaction retains recent complete JSON records and adds a
`telemetry_compacted` omission marker; it can lose global diagnostic records if
the process crashes during compaction. Content marked sensitive remains only
in the per-session `events.jsonl`, which is never rewritten: it keeps its first
64 MiB and then records one `telemetry_session_capped` marker and omits later
records.
Both paths honor `SGT_RUNTIME_STATE_ROOT`. Factory reset closes telemetry handles
while cleaning up, and subsequent events can open fresh logs.

Microphone capture automatically writes `[AudioCapture]` records in development
and release builds. Each stream has an ID and opening/configured/playing/released
events, with one-minute cumulative health summaries (five seconds with verbose
logging). These include the actual
device and Windows default input roles, input inventory, endpoint mute/volume/peak,
sample format/rate/channel count, callback age and suppression, and raw per-channel,
averaged-mono and delivered signal levels. Zero/nonfinite/clipped sample counts
distinguish silent input from conversion problems. At most 32 input devices and
32 channels are detailed; the actual selected channel count is always logged.
Stream errors are persisted. These diagnostics do not store audio or transcripts
and do not change device selection, gain, conversion, or the UI.

Preset `autoPaste` streams into a bound destination. Writable text-range providers
use a verified, replaceable provisional tail. Windows editors without that
capability can use explicitly unverified keyboard revisions; protected targets
remain excluded. Ownership loss abandons the old tail without erasing it, then
permits rebinding after 250 ms of stable foreground and a further 250 ms of valid
captured destination identity/capability, without waiting for a provider
final. Pending events are retained; only the continuing segment tail is routed
to the new destination. Corrections crossing that boundary are delivered there
without rewriting the old editor. Uncertain mutations are not retried. Dedicated
transcription retains the shared speech-end reducer, so speech
pauses finalize chunks without ending capture. See the
[preset audio contract](../.claude/parity/preset-audio.md).
With verbose logging or the text-diagnostics flag below, `[AutoPaste]` records a
session ID, received/dispatched ordering, event kind,
queue depth, text lengths, selected backend, and mutation acceptance/failure.
`[AutoPasteRange]` records preserved-prefix and selected/inserted lengths;
postcondition failures include expected/observed surrounding lengths.
`[AutoPasteKeyboard]` explicitly labels counted Backspace delivery unverified.
Windows dictation does not monitor global keyboard/mouse events or cancel on
input activity. Shortcut modifiers temporarily defer dispatch until release,
without abandoning the provisional tail. Actual destination identity and verified
text-range postconditions are still checked; unverified keyboard delivery cannot
distinguish concurrent user edits from its own output.
These records contain no transcript or document text. A keyboard batch accepted
by Windows is not proof that the destination applied it correctly.
For batch clipboard auto-paste, debug builds record `[AutoPasteBatch]` stages in
`session.log`; set `SGT_AUTOPASTE_BATCH_DIAGNOSTICS=1` to enable the same records
in a release build. The stages identify the captured and routed window, focused
native control, clipboard sequence, focus calls, caret probe, and accepted
Ctrl+V input events. They contain no clipboard or transcript text. Accepted
input events do not prove that the destination inserted the clipboard content.
Capture health also records maximum callback processing time and observed callback
gap in microseconds; these distinguish slow application callbacks from delivery gaps
when investigating audio-buffer discontinuities. They do not measure lost audio directly.
With either diagnostics flag, `[TranscriptionDelivery]` records raw/delivered
lengths, trimmed overlap length, and whether stabilization
changed an update. The shared ten-word / 64-scalar delivery policy applies to
dedicated transcription in presets and Live Translate, including provider finals.
Matched carryover prefixes are trimmed from interims without withholding their new tail. Normal
conclude preserves already delivered provisional text. With the text diagnostics
flag below, `[TranscriptionDeliveryText]` also preserves bounded raw-provider and
delivered text separately; insertion diagnostics then describe delivered text.

For an explicitly consented transcript comparison, launch with
`SGT_AUTOPASTE_TEXT_DIAGNOSTICS=1`. This opt-in adds `[AutoPasteText]` JSON
records to the same bounded local log: raw and sanitized received hypotheses,
plus attempted old/new owned tails keyed by process, session and sequence.
Use the matching acceptance/failure records to distinguish attempted input from
acknowledged dispatch; neither is destination readback. Whitespace and Unicode
are preserved with scalar, UTF-16 and byte counts. Each text field is bounded to
4,096 scalars and explicitly marks truncation. No surrounding document text or
audio is captured. This mode records potentially sensitive dictated content;
leave it unset for ordinary use and review the log before sharing it.
`[ModelDownload]` records verified model-file requests, bounded byte progress,
integrity verification, cancellation, and contextual failures. These records
distinguish an active transfer from a failed or cancelled first-use installation.

After the active frontend assets exist, direct `cargo run` is valid, but
`run-dev.ps1` is preferred because it applies the bounded cache and delivery
channel invariants. Optional mini-app source changes are not visible merely
because a frontend `dist/` changed. Creation is synchronized by
`run-dev.ps1 -UseStagingDelivery`; package and stage other affected components
before launching with that switch.
Do not run `cargo check` again after successful all-target Clippy unless the
target or feature set differs. Do not use a release build as routine
validation; release packaging enables LTO/stripping and rebuilds every
packaged frontend.

### Desktop input regression

Compositor recovery waits for the owning browser job to empty before reusing its
profile. A supervisor scheduling pause grants a fresh heartbeat observation
window; it does not itself prove a renderer hang. Intentional console shutdown
does not restart compositors or report their expected disconnects as failures.

On an unlocked Windows desktop, exercise real result/status controllers against
Explorer context menus, ordinary application input, and each visible taskbar:

```powershell
python scripts/test_desktop_compositor_input.py --exe <development-exe> --expose-desktop
```

The optional `--expose-desktop` switch requires `pywin32`; it minimizes and then
restores open windows. The runner checks empty, visible, dismissed, reopened, and
recreated controllers. It writes a fresh report beneath the managed development
cache's `evidence` directory. An obscured target or missing menu fails acceptance.

Result geometry has a focused live regression for drag/resize cancellation,
settlement, browser zoom, and browser-side revision acknowledgement:

```powershell
node --test src/overlay/result/scene_compositor/geometry_runtime.test.cjs
python scripts/test_result_geometry.py --exe <development-exe>
```

The live runner requires `websocket-client` and an unlocked desktop. It uses
physical pointer input against a separate renderer with an isolated runtime
state root, enables a loopback debugging endpoint only for that process, and
saves screenshots and the tested executable's SHA-256 in a fresh evidence
directory. It restores the cursor and closes its renderer on completion.

### Development-only Windows performance profiles

The normal `release` profile remains the compact shipping baseline. It uses
size optimization, fat LTO, one codegen unit, and symbol stripping. The
comparison lanes are opt-in development experiments and are never part of a
canonical build or release:

```powershell
.\scripts\windows-performance.ps1 -Action Compare
```

`release-compact` reproduces the shipping optimizer, `release-balanced` tests
the size-aware speed optimizer, and `release-perf` is the speed-first upper
bound. The script runs the result, status, and realtime
compositor restart corpus, records elapsed time/working set/thread peaks under
the bounded development cache, hashes all three binaries, and enforces
`scripts/performance-contract.json`. No lane is a release gate. Results are
engineering evidence for a later source change, never work to run again while
publishing. Optimized builds default to two Cargo jobs so concurrent fat-LTO
compilers cannot exhaust ordinary developer machines; use `-BuildJobs 1` when
other large builds are active.

For a profile-guided candidate, install Rust's matching LLVM tools once and
train the balanced profile on the same compositor corpus:

```powershell
rustup component add llvm-tools-preview
.\scripts\windows-performance.ps1 -Action TrainPgo
```

The report, merged profile, and optimized executable stay under
`%LOCALAPPDATA%\SGT-Development\cache\performance`; raw counters and Cargo
targets are removed after validation. They are development evidence only and
are not accepted by the canonical build. Any source or toolchain change makes
the comparison stale.

Attribute retained release bytes before changing a dependency boundary:

```powershell
.\scripts\binary-ownership.ps1
```

The report combines Cargo/PE ownership with de-obfuscated Android DEX packages
and APK entry groups. It stays in the bounded performance cache. The diagnostic
Windows build is explicitly non-shipping and cannot relax the production
component-delivery gate. To capture a focused ETW trace for one compositor:

```powershell
.\scripts\capture-windows-performance.ps1 -Smoke Result
```

WPR refuses to replace another active recording. Trace files stay outside the
repository under the performance cache.

## Frontend development

Each frontend owns its `package.json`. Typical loop:

```powershell
Push-Location screen-record
npm install
npm run dev
npm test
npm run build
Pop-Location
```

Frontend type-checking uses the native TypeScript 7 compiler. Keep `tsconfig`
options compatible with TypeScript 7 instead of suppressing removed-option
diagnostics. The recorder also keeps the TypeScript 6 compatibility package
under the canonical `typescript` dependency solely for tools that still use
the legacy compiler API; its `tsc` executable remains the native compiler.

Use the equivalent package directory for another mini app. Packaged assets are copied to the matching `src/overlay/<feature>/dist/` directory by repository build scripts.

Recorder-specific architecture and tests: [`../screen-record/README.md`](../screen-record/README.md).

Optional runtime, worker, model, and WebView-package changes must follow the
[component delivery contract](COMPONENT_DELIVERY.md). Keep first-use packages
independent, integrity-pinned, registry-owned, and removable.

## Windows x64 target

For target-sensitive changes and release checkpoints, validate the supported
MSVC target through the repository wrapper:

```powershell
.\scripts\validate-windows-targets.ps1
```

The validation log is written to `target/validation-x86_64_pc_windows_msvc.log`.
The wrapper is an explicit-target Cargo check, so routine source edits do not
need it after a successful x64 Clippy run.

## UI design system

The egui shell uses `AppTheme` tokens and the components in `src/gui/widgets.rs`.
Every modal must go through `material_modal` or `ConfirmModal`; do not construct
`egui::Modal` directly in feature code. Keep semantic title, body, action, card,
and state-layer behavior in shared components, while feature modules own only
their content and state transitions. `source_contract_tests` enforces the modal
boundary.

## Retained user collections

Every automatically growing, persistent, user-visible repeating collection must
have a bounded default, automatic pruning, per-item deletion, and a persisted
localized maximum-items control in the UI that displays it. The Result Library,
Computer Control memory, Screen Recorder projects and uploaded backgrounds,
Translation Gummy transcripts, and TTS Playground clips are the reference
implementations. Changing the limit must prune through the same owner that
deletes the collection's managed files; a visual-only or process-memory-only
slider is invalid. Explicitly saved libraries, models, presets, exports, and
reference voices remain durable until the user removes them.
The text-input arrow-key recall buffer shares the main History limit and clear
action instead of creating another settings control.

Invisible derived data such as browser caches, waveforms, diagnostics, staging,
and download scratch space uses fixed code-owned count/byte/age budgets instead
of user sliders. Cleanup must be confined to compiled app-owned roots, reject
reparse points, preserve unknown or modified files, and never delete exports or
other user-created output.

## Android

Android toolchain prerequisites are owned by [`mobile/README.md`](../mobile/README.md).
Windows PowerShell is the canonical development path:

```powershell
cd mobile
.\gradlew.bat :androidApp:testFullDebugUnitTest --console=plain
```

Android startup and common shell movement have a dedicated Baseline/Startup
Profile producer plus Macrobenchmark. Normal builds do not run a device or
generate profiles automatically:

```powershell
cd mobile
.\gradlew.bat :androidApp:generateBaselineProfile --no-parallel --max-workers=1 --console=plain
.\gradlew.bat :baselineprofile:connectedFullBenchmarkReleaseAndroidTest --console=plain
```

An emulator can generate the profiles, but run Macrobenchmark measurements on a
controlled physical device; AndroidX intentionally rejects emulator performance
numbers. The benchmark records cold startup, frame timing, and peak process
memory both without compilation and with the packaged Baseline Profile. The
generated Startup Profile contains only launch-to-ready code; scrolling and
other common shell movement remain in the broader Baseline Profile. Generation
merges both shipping flavors into one profile. Keep the connected flavor tasks
serialized because they install the same package identity on one device.

Use the repository wrapper to build and validate the APK, run the benchmark on
one exact physical device, and enforce the startup/frame/memory contract:

```powershell
.\scripts\android-performance.ps1 -Serial <adb-serial>
```

`-AllowEmulatorDiagnostics` collects non-gating traces only; emulator timings
never satisfy the release performance contract.

The mobile release wrapper also requires the compiled profile entries in the
Full APK and enforces both the APK and profile byte budgets from
`scripts/performance-contract.json`.

Run the Play suite instead for Play-only delivery work. Run both flavor suites
at cross-flavor and release checkpoints. Assemble a debug APK only when it is
needed for installation; test and assemble tasks already compile their
variants, so a separate `compile*DebugKotlin` pass is redundant.

WSL delegates to the Windows toolchain:

```bash
./mobile/scripts/sgtp-wsl.sh build
./mobile/scripts/sgtp-wsl.sh install
./mobile/scripts/sgtp-wsl.sh run
./mobile/scripts/sgtp-wsl.sh gradle :androidApp:testFullDebugUnitTest --console=plain
```

See [`../mobile/README.md`](../mobile/README.md) for flavors, device setup, and release artifacts.

## Locale catalogs

Desktop and Android locale catalogs are split into typed subsystem bundles. Run the
section-aware, non-writing parity check after changing locale schema or copy:

```powershell
node scripts/i18n_scan.mjs --self-test
node scripts/i18n_scan.mjs --check
```

Run `node scripts/i18n_scan.mjs` without `--check` only when intentionally refreshing
the tracked `scripts/i18n_scan_report.json` audit artifact.

## Help index

Build the Help Assistant index with Gemini Embedding 2 over a committed SGT
source revision. The corpus includes Windows and Android code, shared UI,
build configuration, tests, and product docs. The builder excludes local
credentials, generated output, vendor dependencies, and binary assets. It
redacts credentials embedded in URLs before sending source chunks to Gemini.
Put the build keys in the ignored root `.env` or set the same names in the
environment. The pool uses `GEMINI_API_KEY` and numbered slots such as
`GEMINI_API_KEY_2`.

```powershell
$helpRevision = (git rev-parse HEAD).Trim()
$helpRoot = Join-Path $env:LOCALAPPDATA 'SGT-Development\cache\help-index'
$helpPackage = Join-Path $helpRoot 'package'
$helpEmbeddings = Join-Path $helpRoot 'embeddings.sqlite3'
py -3 scripts/help_index_build.py --ref $helpRevision --output-dir $helpPackage --embedding-cache $helpEmbeddings
```

Reuse the same SQLite cache across revisions to embed only new or changed
chunks. To resume an interrupted build, rerun the command with the same
revision and cache. The builder writes a content-addressed gzip asset and
`help-index-v2.package.json`. The asset header records the source revision.

To spot-check retrieval during development, query the local package with a
product question:

```powershell
$helpManifest = Get-Content "$helpPackage\help-index-v2.package.json" -Raw | ConvertFrom-Json
$helpIndex = Join-Path $helpPackage $helpManifest.helpIndex.asset
py -3 scripts/help_index_query.py --index $helpIndex --platform windows "How do I change which model is tried first?"
```

The package contains source text. Once the indexed source is public, stage it
through the normal component delivery flow:

```powershell
py -3 scripts/component_release.py stage `
  --package-manifest "$helpPackage\help-index-v2.package.json" `
  --tracked-manifest component-delivery/help-index-v1.json `
  --contract-relative component-delivery/help-index-v1.json `
  --asset-root $helpPackage
```

Use `SGT_COMPONENT_DELIVERY_CHANNEL=staging` and the generated staging contract
root to verify that both Windows and Android load and cache the candidate.
Then follow the [promotion checkpoint](RELEASING.md#promote-optional-component-candidates)
to publish the verified bytes and update the tracked contract. The contract
schema selects the index reader. Existing released hosts keep the contract
compiled into their version.

## Documentation

Check local Markdown links and the Claude redirect:

```powershell
py -3 scripts\check-docs.py
```

Keep one owner per fact. Link to source/configuration for volatile versions, model IDs, and runtime assets instead of copying them into prose.

## Release builds

The release wrapper builds Windows x64 only:

```powershell
.\build.ps1
```

The artifact remains under the x64 target release directory. Full release checklist: [`RELEASING.md`](RELEASING.md).
