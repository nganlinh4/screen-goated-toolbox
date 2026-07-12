# SGT overlay for qwen3-asr-rs

This directory is SGT's vendored fork of
[`second-state/qwen3_asr_rs`](https://github.com/second-state/qwen3_asr_rs),
package version `0.2.0`. Keep the upstream `README.md` as upstream
documentation; put SGT-specific notes here.

SGT uses the library in two artifacts:

- Active backend: [`native/qwen3_runtime`](../../native/qwen3_runtime/README.md),
  a DLL loaded in process by the Windows app.
- Reference artifact:
  [`native/qwen3_reference_sidecar`](../../native/qwen3_reference_sidecar/README.md),
  a standalone server retained for comparison and diagnostics.

Build them from the repository root:

```powershell
.\scripts\build_qwen3_runtime.ps1
.\scripts\build_qwen3_reference_sidecar.ps1
```

Do not link the standalone server into the main executable or describe it as
the active Screen Recorder backend.
