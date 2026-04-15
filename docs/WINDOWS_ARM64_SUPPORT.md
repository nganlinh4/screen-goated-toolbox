# Windows ARM64 Support

This repo now supports compiling and packaging both Windows `x64` and Windows `arm64` builds.

## What is validated

- `cargo check --target x86_64-pc-windows-msvc`
- `cargo check --target aarch64-pc-windows-msvc`
- Architecture-aware runtime/tool downloads for:
  - ONNX Runtime + DirectML local runtime assets
  - `ffmpeg`
  - `deno`
- Architecture-aware installer and updater asset selection

## What compile success does and does not mean

Passing `cargo check` for `aarch64-pc-windows-msvc` means the Rust codebase and its native dependencies can be compiled for Windows ARM64.

It does **not** guarantee full feature parity on all Windows-on-Arm environments, especially Apple-silicon virtual machines.

## Known runtime caveats

### WebView2-dependent features

These features require Microsoft Edge WebView2 Runtime at runtime:

- realtime overlay
- bilingual relay
- text input overlay
- preset wheel
- PromptDJ
- screen record UI
- tray popup web UI

The app now detects missing WebView2 and prompts the user instead of failing more opaquely during warmup.

### Qwen3 local runtime

Qwen3 local runtime remains unavailable on Windows ARM64 / Apple-silicon Windows VMs.

Reasons:

- current runtime is shipped only for x64 Windows
- current runtime requires NVIDIA CUDA hardware

The UI now reports this explicitly instead of attempting installation and failing later.

### GPU / VM feature limitations

Compilation support does not imply that all GPU-backed runtime paths work inside a VM.

Higher-risk areas on Apple-silicon Windows VMs:

- DirectML-backed local inference
- Windows Graphics Capture / recorder behavior
- GPU export and encode paths

Basic app launch and WebView-driven surfaces are now much more supportable, but advanced GPU-dependent features still depend on what the VM exposes.

## Windows validation workflow

Use:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\validate-windows-targets.ps1 -Arch all
```

Per-arch:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\validate-windows-targets.ps1 -Arch x64
powershell -ExecutionPolicy Bypass -File scripts\validate-windows-targets.ps1 -Arch arm64
```

Logs are written to:

- `target\validation-x86_64_pc_windows_msvc.log`
- `target\validation-aarch64_pc_windows_msvc.log`
