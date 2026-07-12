# Offline TTS DLL ABI

`src/api/realtime_audio/tts_libtorch_runtime.rs` loads model-specific Windows
DLLs through one C ABI. Despite the loader's historical `libtorch` name, the
ABI does not require a particular inference framework.

## Current status

- Voxtral is the only configured consumer.
- `native/sgt_tts_runtime/` is a Rust prototype that implements the ABI by
  starting `native/sgt_tts_runtime_py/synthesize.py` for each request.
- No `native/voxtral_runtime/dist/sgt_voxtral_runtime.dll` is committed.
  Therefore the configured clean-install download cannot currently succeed.
- The app's Voxtral installer still requires shared libtorch DLLs, while the
  prototype DLL delegates inference to Python. Treat this path as unfinished,
  not as a published runtime.

See [`voxtral_runtime/README.md`](voxtral_runtime/README.md) before changing or
publishing the Voxtral path.

## ABI version 1

All strings are UTF-8 byte slices. Every function uses the C ABI and an
unmangled export name.

```c
uint32_t sgt_tts_runtime_version(void);

int32_t sgt_tts_create(
    const char* model_dir_utf8,
    size_t model_dir_len,
    void** out_runtime);

int32_t sgt_tts_destroy(void* runtime);

int32_t sgt_tts_synthesize(
    void* runtime,
    const char* text_utf8,
    size_t text_len,
    const char* voice_utf8,
    size_t voice_len,
    const char* lang_utf8,
    size_t lang_len,
    float speed,
    const int16_t** out_pcm16,
    size_t* out_pcm_count,
    int32_t* out_sample_rate);

int32_t sgt_tts_free_audio(void* runtime, const int16_t* pcm16);

int32_t sgt_tts_last_error(
    void* runtime,
    const char** out_message,
    size_t* out_len);
```

There are six exports. `sgt_tts_runtime_version()` must return `1`. Other
functions return `0` on success and a negative status on failure.

The runtime owns returned mono PCM16 audio until the caller passes the same
pointer to `sgt_tts_free_audio`. The error string is borrowed runtime storage;
the caller must copy it before another mutating call.

The host serializes synthesis for a cached handle. Destruction must not race
with any call on that handle.

## Validation

The ABI implementation has symbol, dispatch, and loader-compatibility tests:

```powershell
cargo test --manifest-path native/sgt_tts_runtime/Cargo.toml
```

Tests that need an installed DLL skip when it is absent. Set
`SGT_TTS_RUNTIME_DLL` to test a specific built DLL. Publishing also requires a
real model synthesis test; symbol-loading alone is not sufficient.
