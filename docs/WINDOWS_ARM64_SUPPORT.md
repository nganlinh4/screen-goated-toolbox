# Windows ARM64 Boundary

The repository compiles and packages Windows x64 and ARM64. Public GitHub releases currently ship x64; ARM64 remains a source and validation target until its release boundary is approved.

## What Validation Proves

`scripts/validate-windows-targets.ps1` checks both MSVC targets and architecture-sensitive asset selection. A successful ARM64 check proves that Rust code and native dependencies compile for that target. It does not prove every hardware-backed feature works on every Windows-on-Arm machine or VM.

See [Development](DEVELOPMENT.md#windows-targets) for commands and log paths.

## Runtime Boundaries

### WebView2

All WebView-backed surfaces require Microsoft Edge WebView2 Runtime. On startup, a missing runtime starts a background bootstrapper install; successful installation restarts the app. Feature launch remains guarded while installation is incomplete. Source: `src/main.rs` and `src/runtime_support.rs`.

### Qwen3-ASR

The shipped Qwen3 local runtime is x64-only and requires NVIDIA CUDA. It is unavailable to native ARM64 processes and Apple-silicon Windows VMs; the app reports that boundary before installation.

### GPU and VM Features

Availability still depends on the machine or VM exposing the required Windows/GPU stack. Test these on target hardware:

- DirectML local inference
- Windows Graphics Capture
- recorder preview/export and hardware encoding
- architecture-specific helper downloads and updates

Compile success alone is not a runtime-parity claim.
