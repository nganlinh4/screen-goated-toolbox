# Archived Qwen3-ASR reference sidecar

This directory publishes the standalone `asr-server.exe` built from the
vendored `third_party/qwen3-asr-rs` source.

It is retained only as reference and diagnostic source. Realtime transcription
uses [`native/qwen3_runtime`](../qwen3_runtime/README.md), not this process.
The shipped app has no route, download action, or Downloaded Tools entry for
this executable.

Build it from the repository root in Windows PowerShell:

```powershell
.\scripts\build_qwen3_reference_sidecar.ps1
```

For local diagnostics only, the script:

- builds the vendored `asr-server` binary;
- copies it to `native/qwen3_reference_sidecar/dist/asr-server.exe`; and
- creates `dist/qwen3-asr-reference-windows-x64.zip` with its matching
  libtorch runtime.

The build supports CPU, CUDA 12.6, and CUDA 12.8 variants through `-Runtime`.
It does not upload artifacts.
