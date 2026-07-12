# Qwen3-ASR reference sidecar

This directory publishes the standalone `asr-server.exe` built from the
vendored `third_party/qwen3-asr-rs` source.

It is a reference and diagnostic artifact. Realtime transcription and Screen
Recorder Qwen Local subtitles use
[`native/qwen3_runtime`](../qwen3_runtime/README.md), not this process. The
Downloaded Tools UI still manages the standalone server, so do not silently
remove it without removing that product surface too.

Build it from the repository root in Windows PowerShell:

```powershell
.\scripts\build_qwen3_reference_sidecar.ps1
```

The script:

- builds the vendored `asr-server` binary;
- copies it to `native/qwen3_reference_sidecar/dist/asr-server.exe`; and
- creates `dist/qwen3-asr-reference-windows-x64.zip` with its matching
  libtorch runtime.

The build supports CPU, CUDA 12.6, and CUDA 12.8 variants through `-Runtime`.
It does not upload artifacts.
