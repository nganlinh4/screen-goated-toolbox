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

```powershell
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

After frontend assets exist, direct `cargo run` is valid for a checkout without
the private creation runtime. When that private checkout is present, prefer
`run-dev.ps1` so its sidecar stays synchronized. Do not use a release build as
routine validation; release packaging enables LTO/stripping and rebuilds every
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

## Windows targets

Validate MSVC targets through the repository wrapper:

```powershell
.\scripts\validate-windows-targets.ps1 -Arch x64
.\scripts\validate-windows-targets.ps1 -Arch arm64
.\scripts\validate-windows-targets.ps1 -Arch all
```

ARM64 validation needs LLVM `clang` under `C:\Program Files\LLVM\bin`. Logs are written to `target/validation-*.log`. Runtime limitations are tracked in [`WINDOWS_ARM64_SUPPORT.md`](WINDOWS_ARM64_SUPPORT.md).

## Android

Android uses JDK 17 and Android SDK platform/build tools configured by Gradle. Windows PowerShell is the reliable path on this workstation:

```powershell
cd mobile
.\gradlew.bat :androidApp:assembleFullDebug --console=plain
```

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
