# Screen Goated Toolbox

Screen Goated Toolbox (SGT) is a Windows-first AI productivity app with an Android companion. It combines visual presets, screen/audio capture, live transcription and translation, text-to-speech, computer control, media tools, and a GPU-backed screen recorder in one native desktop shell.

## What ships

### Windows desktop

- Visual preset graph for screen, text, microphone, and system-audio workflows.
- Cloud and local text, vision, speech, translation, and TTS providers.
- Live transcription/translation overlays with microphone, device, and per-app capture.
- Gemini Live computer control with screen context, browser bridge, typed OS tools, and optional app integrations.
- Screen recorder/editor with timeline effects, camera zoom, cursor rendering, backgrounds, subtitles, narration, and GPU export.
- Embedded mini apps for focused media and creative workflows.
- Preset wheel, favorite bubble, history, clipboard, and result overlays.

### Android companion

- Live capture, transcription, translation, and TTS.
- Native Compose UI plus optional overlay support in the full flavor.
- Shared Windows-derived preset/runtime contracts where parity applies.
- Play flavor for Google Play distribution.

Exact models and provider defaults change often. Canonical model data lives in [`catalog/model_catalog.json`](catalog/model_catalog.json), not this README.

## Install

### Windows

Download the current installer/executable from [GitHub Releases](https://github.com/nganlinh4/screen-goated-toolbox/releases).

- x64: `ScreenGoatedToolbox_v<VERSION>.exe`

Public releases currently ship Windows x64. ARM64 source compilation/packaging is available but remains a validation target with runtime caveats; see [`docs/WINDOWS_ARM64_SUPPORT.md`](docs/WINDOWS_ARM64_SUPPORT.md).

WebView-based surfaces require the [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

### Android

Install the Play flavor from [Google Play](https://play.google.com/store/apps/details?id=dev.screengoated.toolbox.mobile). Development/full-flavor APK workflow is documented in [`mobile/README.md`](mobile/README.md).

## Build from source

Windows prerequisites:

- Current stable Rust toolchain with MSVC target.
- Visual Studio 2022 Build Tools, Desktop development with C++ workload.
- Node.js + npm for embedded web frontends.
- WebView2 Runtime.
- LLVM/Clang plus ARM64 MSVC components only for Windows ARM64 builds.

```powershell
git clone https://github.com/nganlinh4/screen-goated-toolbox.git
cd screen-goated-toolbox

# Build required embedded frontends, then run Rust
.\run-dev.ps1
```

Release packaging is owner-controlled:

```powershell
.\build.ps1 -Arch x64
.\build.ps1 -Arch arm64
.\build.ps1 -Arch all
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for validation, frontend, Android, and help-index workflows.

## Repository map

| Path | Owner |
|---|---|
| `src/` | Windows Rust application |
| `catalog/` | Canonical model catalog |
| `screen-record/` | Recorder/editor frontend |
| `promptdj-midi/` | PromptDJ frontend |
| `translation-gummy-ui/` | Translation Gummy frontend |
| `tts-playground-ui/` | TTS Playground frontend |
| `mobile/` | Android/Kotlin Multiplatform project |
| `native/` | SGT-owned native runtimes and sidecars |
| `parity-fixtures/` | Cross-platform golden data |
| `.claude/parity/` | Windows/Android parity contracts |
| `third_party/` | Vendored upstream code; preserve upstream docs |

## Documentation

- [Development](docs/DEVELOPMENT.md)
- [Release process](docs/RELEASING.md)
- [Computer Control development contract](docs/COMPUTER_CONTROL_DEVELOPMENT.md)
- [Windows ARM64 boundary](docs/WINDOWS_ARM64_SUPPORT.md)
- [Mobile workflow](mobile/README.md)
- [Screen recorder frontend](screen-record/README.md)
- [Native runtimes](native/README.md)
- [Browser-control extension](src/overlay/computer_control/browser_ext/README.md)

## Support

Vietnamese users can support development through [VietQR](https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT).

## Author

Developed by [nganlinh4](https://github.com/nganlinh4). No repository license file is currently published; do not assume reuse terms beyond explicit upstream licenses in vendored components.
