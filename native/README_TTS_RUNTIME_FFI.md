# sgt_tts_runtime FFI contract (shared by all libtorch-shim TTS DLLs)

Every offline TTS model that runs through `src/api/realtime_audio/tts_libtorch_runtime.rs`
must export the same C ABI from its `sgt_<model>_runtime.dll`. The current
DLLs are:

| Model | DLL path (committed to `main`) | Rust worker |
| --- | --- | --- |
| Step Audio EditX | `native/step_audio_runtime/dist/sgt_step_audio_runtime.dll` | `src/api/tts/worker/worker_step_audio.rs` |
| Mistral Voxtral 4B TTS | `native/voxtral_runtime/dist/sgt_voxtral_runtime.dll` | `src/api/tts/worker/worker_voxtral.rs` |

Each DLL links statically against the **shared libtorch installation** that
`src/api/realtime_audio/qwen3/runtime.rs` already downloads on first use
(`libtorch-win-shared-with-deps-2.7.1+cu128`). There is **no second libtorch
copy** — the loader sets the private-bin dir as the DLL search path so the
`torch_*.dll` / `c10*.dll` resolve from the same location across all models.

## ABI version

```c
#define SGT_TTS_RUNTIME_ABI_VERSION 1u
```

`sgt_tts_runtime_version()` must return exactly this value. The Rust loader
refuses to use the DLL on mismatch.

## Required exports (all `extern "C"`, all strings are UTF-8)

```c
uint32_t sgt_tts_runtime_version(void);

int32_t sgt_tts_create(
    const char* model_dir_utf8, size_t model_dir_len,
    void** out_runtime);

int32_t sgt_tts_destroy(void* runtime);

int32_t sgt_tts_synthesize(
    void* runtime,
    const char* text_utf8,   size_t text_len,
    const char* voice_utf8,  size_t voice_len,  // may be empty
    const char* lang_utf8,   size_t lang_len,   // BCP-47, may be empty
    float       speed,                          // 1.0 = natural
    const int16_t** out_pcm16,                  // little-endian mono PCM
    size_t*         out_pcm_count,              // # of int16_t
    int32_t*        out_sample_rate);           // Hz

int32_t sgt_tts_free_audio(void* runtime, const int16_t* pcm16);

int32_t sgt_tts_last_error(void* runtime, const char** out_message, size_t* out_len);
```

Return `0` on success, negative on failure. The PCM buffer ownership stays
with the runtime until `sgt_tts_free_audio` is called.

## Calling sequence per request

```
1. sgt_tts_create(model_dir, &h)      // once per process, cached in Rust
2. for each utterance:
     sgt_tts_synthesize(h, text, voice, lang, speed, &pcm, &n, &sr)
     // copy samples
     sgt_tts_free_audio(h, pcm)
3. sgt_tts_destroy(h)                  // at shutdown
```

## Threading

- `sgt_tts_synthesize` may be called from any thread, but **not concurrently**
  on the same handle. The Rust worker takes the handle under a Mutex.
- `sgt_tts_destroy` must not be called concurrently with any other call on
  the same handle.

## Build instructions (CMake skeleton)

```cmake
cmake_minimum_required(VERSION 3.18)
project(sgt_<model>_runtime LANGUAGES CXX)

# Point at the libtorch unpacked by qwen3 install
# (download once: https://download.pytorch.org/libtorch/cu128/...)
set(CMAKE_PREFIX_PATH "<path-to-libtorch>")
find_package(Torch REQUIRED)

add_library(sgt_<model>_runtime SHARED
    src/exports.cc
    src/<model>_impl.cc)
target_link_libraries(sgt_<model>_runtime PRIVATE ${TORCH_LIBRARIES})
target_compile_definitions(sgt_<model>_runtime PRIVATE
    SGT_TTS_RUNTIME_ABI_VERSION=1)
set_target_properties(sgt_<model>_runtime PROPERTIES
    CXX_STANDARD 17
    WINDOWS_EXPORT_ALL_SYMBOLS OFF
    LINK_FLAGS "/DEF:src/exports.def")
```

`exports.def` lists exactly the eight symbols above.

After building the Release `.dll`, commit it to
`native/<model>_runtime/dist/sgt_<model>_runtime.dll`. The Rust side fetches
it via `https://raw.githubusercontent.com/<owner>/<repo>/main/native/<model>_runtime/dist/sgt_<model>_runtime.dll`
the first time the user selects the matching TTS method.

## Model-specific notes

- **Step Audio EditX** — 3B PyTorch checkpoint. Implement the autoregressive
  text→audio path described in
  [stepfun-ai/Step-Audio-EditX](https://github.com/stepfun-ai/Step-Audio-EditX).

- **NVIDIA Magpie-Multilingual 357M** does not use this FFI. It runs through
  the managed Python/NeMo sidecar documented in `native/magpie_runtime/`.

- **Mistral Voxtral 4B TTS** — Open weights under CC BY-NC 4.0. Implement
  the inference per the model card on
  [mistralai/Voxtral-4B-TTS-2603](https://huggingface.co/mistralai/Voxtral-4B-TTS-2603).
