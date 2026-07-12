# Qwen3-ASR native runtime

This DLL is the active local Qwen3-ASR backend for realtime transcription and
Screen Recorder subtitles.

## Architecture

- DLL source: `native/qwen3_runtime/`
- Inference library: `third_party/qwen3-asr-rs/`
- Host loader and installer: `src/api/realtime_audio/qwen3/runtime.rs`
- Published files: `native/qwen3_runtime/dist/sgt_qwen3_runtime.dll` and
  `sgt_qwen3_runtime.manifest.json`

The host verifies DLL size, SHA-256, and ABI version before loading it. ABI
version is defined in `src/protocol.rs`; it is currently `2`. The runtime
accepts 16 kHz mono PCM16 and requires an NVIDIA CUDA-capable GPU. There is no
CPU fallback.

The managed install places the runtime DLL, its manifest, and required
libtorch/CUDA DLLs in
`%LOCALAPPDATA%\screen-goated-toolbox\bin\x64`. Development discovery also
checks the runtime crate's release output and `dist/qwen3-runtime-windows-x64/`.

## Build and package

Run the repository script from Windows PowerShell:

```powershell
.\scripts\build_qwen3_runtime.ps1
```

Useful options:

```powershell
.\scripts\build_qwen3_runtime.ps1 -Runtime cu126
.\scripts\build_qwen3_runtime.ps1 -Runtime cu128 -CopyToPrivateBin
.\scripts\build_qwen3_runtime.ps1 -Clean
```

`-Runtime auto` selects a CUDA package from the detected NVIDIA GPU. The script
builds the DLL, refreshes the committed DLL and manifest, assembles a bundle
with libtorch dependencies, and writes
`dist/qwen3-runtime-windows-x64.zip`. It does not upload artifacts.

After a runtime change, verify the committed manifest matches the committed
DLL and exercise transcription through the desktop host. The standalone
reference server is a separate diagnostic artifact documented in
[`../qwen3_reference_sidecar/README.md`](../qwen3_reference_sidecar/README.md).
