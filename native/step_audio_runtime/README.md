# sgt_step_audio_runtime.dll

Custom libtorch shim driving Step Audio EditX inference for offline TTS.

- **FFI contract**: see [`../README_TTS_RUNTIME_FFI.md`](../README_TTS_RUNTIME_FFI.md)
- **Rust caller**: `src/api/tts/worker/worker_step_audio.rs`
- **Model weights**: downloaded by `src/api/realtime_audio/step_audio_assets.rs`
  into `%APPDATA%/screen-goated-toolbox/models/step_audio_editx/`
  (HF primary, ModelScope fallback).
- **Upstream**: <https://github.com/stepfun-ai/Step-Audio-EditX>

Commit the built Release DLL to `./dist/sgt_step_audio_runtime.dll`.
