# Development

Use this document for commands and repository structure. AI-specific invariants live in [`../AGENTS.md`](../AGENTS.md); subsystem contracts live beside their code.

## Desktop setup

Required on Windows:

- Current stable Rust + Cargo.
- Visual Studio 2022 Build Tools with Desktop development with C++.
- Node.js + npm for embedded frontends.
- Microsoft Edge WebView2 Runtime.

Fresh clones must build embedded frontends before Rust because packaged `dist/` assets are not tracked:

```powershell
.\run-dev.ps1
```

Useful options:

```powershell
.\run-dev.ps1 -SkipFrontendBuild
.\run-dev.ps1 -SkipNpmInstall
.\run-dev.ps1 -SkipCreationRuntimeBuild
.\run-dev.ps1 -CargoCommand test
```

When the separately tracked private creation-runtime checkout is present,
`run-dev.ps1` builds its debug sidecar before launching the desktop host. The
script stops if a running process keeps that executable locked, preventing a
stale sidecar from being selected silently. Use
`-SkipCreationRuntimeBuild` only when the private runtime was already built
from the current source. Cargo output is written under `target/dev-run-logs/`.

## Rust validation

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

After the active frontend assets exist, direct `cargo run` is valid for a
checkout without the private Image-to-3D runtime. When that private checkout is
present, prefer `run-dev.ps1` so its sidecar stays synchronized. Archived
Image-to-SVG and image creation/editing sources are not built or synchronized.
Do not run `cargo check` again after successful all-target Clippy unless the
target or feature set differs. Do not use a release build as routine
validation; release packaging enables LTO/stripping and rebuilds every
packaged frontend.

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

Android uses JDK 17 and Android SDK platform/build tools configured by Gradle. Windows PowerShell is the reliable path on this workstation:

```powershell
cd mobile
.\gradlew.bat :androidApp:testFullDebugUnitTest --console=plain
```

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

The in-app help assistant consumes the tracked `help-index.json`. Rebuild requires a KaLM-compatible endpoint accepting `POST /api/embed` with `{"input":"..."}` and returning `{"embeddings":[[...]]}`.

```powershell
$env:KALM_EMBED_SERVER_URL = 'http://127.0.0.1:8400/api/embed'
python scripts/help_index_build.py
python scripts/help_index_query.py --no-llm "question"
```

Without `--no-llm`, the query helper also needs `GEMINI_API_KEY`.

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
