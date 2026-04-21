SGT owns this vendored copy of `qwen3_asr_rs` as the reference sidecar source for the Windows Qwen realtime backend.

Source provenance:
- Upstream: `https://github.com/second-state/qwen3_asr_rs`
- Vendored from upstream `v0.2.0`

Packaging path:
- Build and bundle with [build_qwen3_reference_sidecar.ps1](/mnt/c/work/screen-goated-toolbox/scripts/build_qwen3_reference_sidecar.ps1)
- Standalone downloadable executable is emitted to `native/qwen3_reference_sidecar/dist/asr-server.exe`

Runtime role:
- This sidecar is the current SGT-owned reference backend for `qwen3-asr-turboquant`
- It is separate from the future native TurboQuant runtime and should not be linked into the main SGT executable
