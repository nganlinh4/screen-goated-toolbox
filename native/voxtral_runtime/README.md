# sgt_voxtral_runtime.dll

Custom libtorch shim driving Mistral Voxtral 4B TTS inference.

- **FFI contract**: see [`../README_TTS_RUNTIME_FFI.md`](../README_TTS_RUNTIME_FFI.md)
- **Rust caller**: `src/api/tts/worker/worker_voxtral.rs`
- **Model weights**: downloaded by `src/api/realtime_audio/voxtral_assets.rs`
  into `%APPDATA%/screen-goated-toolbox/models/voxtral_tts_2603/`
  (HF primary, ModelScope fallback).
- **Upstream**: <https://huggingface.co/mistralai/Voxtral-4B-TTS-2603>
- **License**: model weights are CC BY-NC 4.0 — non-commercial use only.

Commit the built Release DLL to `./dist/sgt_voxtral_runtime.dll`.
