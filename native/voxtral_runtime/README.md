# Voxtral TTS runtime status

This integration is experimental and incomplete.

The desktop app currently expects
`dist/sgt_voxtral_runtime.dll`, downloads that filename from the repository,
and also installs shared CUDA libtorch DLLs. This directory contains no runtime
DLL. A clean installation therefore cannot complete the configured Voxtral
runtime download.

The only implementation source is the generic prototype in
`native/sgt_tts_runtime/`. That DLL follows the shared
[`sgt_tts_*` ABI](../README_TTS_RUNTIME_FFI.md), but it launches
`native/sgt_tts_runtime_py/synthesize.py` and depends on a usable local Python
environment plus Mistral inference packages. It is not the self-contained
libtorch runtime described by the old documentation.

Do not publish this integration as working until one coherent deployment path
is chosen and tested end to end. At minimum:

1. Build a DLL named `sgt_voxtral_runtime.dll` that implements ABI version 1.
2. Package every inference dependency the DLL actually needs.
3. Make the installer checks match that package.
4. Commit the DLL at the configured `dist/` path and verify its download URL.
5. Run real synthesis through the desktop worker on a clean machine.

Model weights are handled separately by
`src/api/realtime_audio/voxtral_assets.rs`. Check the upstream model's license
before distributing weights or a runtime bundle.
