# Qwen3-ASR native runtime

This DLL is the active local Qwen3-ASR backend for realtime transcription and
Screen Recorder subtitles.

## Architecture

- DLL source: `native/qwen3_runtime/`
- Inference library: `third_party/qwen3-asr-rs/`
- Host loader and installer: `src/api/realtime_audio/qwen3/runtime.rs`
- Component owner: `src/component_registry/qwen_runtime.rs`

The runtime accepts 16 kHz mono PCM16 and requires an NVIDIA CUDA-capable GPU.
There is no CPU fallback. ABI version is defined in `src/protocol.rs`; it is
currently `2`.

The host downloads three independently verified, content-addressed archives:
one SGT runtime/notices archive and two libtorch/CUDA parts. Their exact files
are installed under
`%LOCALAPPDATA%\screen-goated-toolbox\components\qwen3-cuda-runtime\<version>\bin\x64`.
The component registry verifies archive and per-file size/SHA-256, x64 PE
identity, the complete file tree, and the ownership receipt. It retains file
locks and component leases for the full loaded-runtime lifetime. Downloaded
Tools owns status and removal.

## Build and package

Build the native DLL from Windows PowerShell:

```powershell
.\scripts\build_qwen3_runtime.ps1
```

Useful source-build options:

```powershell
.\scripts\build_qwen3_runtime.ps1 -Runtime cu126
.\scripts\build_qwen3_runtime.ps1 -Runtime cu128 -CopyToPrivateBin
.\scripts\build_qwen3_runtime.ps1 -Clean
```

`-Runtime auto` selects a CUDA package from the detected NVIDIA GPU. For local
debug discovery, set `SGT_QWEN3_RUNTIME_DEV_DIR` to an x64 directory containing
the runtime DLL and its required libtorch DLLs.

Prepare the reviewed split delivery packages separately:

```powershell
.\scripts\build-qwen3-runtime-pack.ps1
```

This produces the deterministic local package inventory under
`local-runtime-bundles/sgt_qwen3_runtime/`. It does not upload artifacts.
Publishing requires append-only upload, remote read-back verification with
`scripts/verify_qwen3_runtime_release.py`, and only then a release host build.

After a runtime change, exercise transcription through the desktop host. The
standalone reference server is archived diagnostic source documented in
[`../qwen3_reference_sidecar/README.md`](../qwen3_reference_sidecar/README.md).
