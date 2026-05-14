# TTS Runtime Parity

## Canonical Source
- Windows runtime entrypoint: [src/api/tts/mod.rs](../../src/api/tts/mod.rs)
- Windows manager queue + interrupt model: [src/api/tts/manager.rs](../../src/api/tts/manager.rs)
- Windows provider workers: [src/api/tts/worker.rs](../../src/api/tts/worker.rs)
- Windows audio player: [src/api/tts/player.rs](../../src/api/tts/player.rs)
- Windows Gemini websocket transport: [src/api/tts/websocket.rs](../../src/api/tts/websocket.rs)
- Windows TTS types + request modes: [src/api/tts/types.rs](../../src/api/tts/types.rs)
- Windows Edge voice catalog: [src/api/tts/edge_voices.rs](../../src/api/tts/edge_voices.rs)
- Windows config schema: [src/config/types/tts.rs](../../src/config/types/tts.rs)
- Windows realtime consumer queue: [src/overlay/realtime_webview/state.rs](../../src/overlay/realtime_webview/state.rs)
- Windows realtime enqueue trigger: [src/overlay/realtime_webview/wndproc.rs](../../src/overlay/realtime_webview/wndproc.rs)
- Windows global TTS settings UI: [src/gui/settings_ui/global/tts_settings.rs](../../src/gui/settings_ui/global/tts_settings.rs)

## Behavior Contract
- Mobile must keep the Windows split between:
  - a global TTS manager
  - provider workers
  - a single audio playback lane
  - feature-local higher-level queues such as realtime Read/TTS
- Global manager behavior:
  - every request gets a unique request ID
  - `speak` appends to the global queue
  - `speak_realtime` appends to the global queue with realtime playback semantics
  - `speak_interrupt` increments interrupt generation, clears queued work, cuts current playback, and starts the new request immediately
  - `stop` increments interrupt generation and clears all queued work
- Realtime Read/TTS behavior:
  - only newly committed translation text is enqueued
  - enabling realtime TTS on an already-long committed buffer skips to the last previous sentence boundary instead of reading the whole history
  - hiding the translation surface stops speech immediately
  - disabling realtime TTS resets queued/spoken realtime offsets
  - realtime auto-speed is based on queued realtime backlog, not on total transcript length
- Provider behavior:
  - Gemini Live uses websocket setup -> setupComplete -> text turn -> streamed audio
  - Edge TTS uses websocket config -> SSML -> streamed MP3 -> decode -> shared player
  - Edge pitch/rate/volume shaping belongs to SSML; the shared player must not apply Edge volume a second time
  - Google Translate TTS uses HTTP MP3 -> decode -> shared player
  - Kokoro 82M v1.0 runs fully offline: the worker drives sherpa-onnx's `SherpaOnnxCreateOfflineTts` against the downloaded `model.onnx` + `voices.bin` + `tokens.txt` + `espeak-ng-data/` bundle, producing 24 kHz float32 samples that get converted to PCM16 LE and chunked into `AudioEvent::Data` for the shared player. No network round-trip per request.
  - Step Audio EditX, NVIDIA Magpie-Multilingual, and Mistral Voxtral 4B run through the **shared libtorch-shim DLL pattern** mirroring `qwen3/runtime.rs`: each model has a custom `sgt_<model>_runtime.dll` committed under `native/<model>_runtime/dist/` and pulled via `raw.githubusercontent.com` on first use. All DLLs implement the `sgt_tts_runtime_*` C ABI documented in `native/README_TTS_RUNTIME_FFI.md` (single ABI version 1) and reuse the libtorch DLLs the Qwen3 runtime already installs. The worker emits a clear "DLL not available — build it per native/<model>_runtime/README.md" notice until the binary lands; once present, every request runs fully offline through `synthesize() -> int16 LE PCM`.
  - All open-weights workers must emit PCM16 mono at 24 kHz (the same rail Gemini and Edge ride). Higher source rates are resampled in `super::resample_audio`.
- Asset download contract (Kokoro is the canonical example; future offline TTS providers must follow it):
  - Files land in `dirs::data_dir()/screen-goated-toolbox/models/<id>/` (e.g. `models/kokoro_v1/`).
  - Primary host is Hugging Face; the worker must try a **ModelScope mirror as a per-file fallback** so gated/region-restricted repos still install on first try.
  - phonemizer / aux data shipped as `.tar.bz2` is extracted via the OS-bundled `tar` binary; the archive is removed after extraction.
  - `is_<model>_downloaded()` checks one mandatory file per category (weights, voices/embeddings, tokenizer, phonemizer marker) rather than a single sentinel — partial installs from cancelled downloads must read as "missing".
- Global settings behavior:
  - methods are `Gemini Live`, `Edge TTS`, `Google Translate`, plus the remaining open-weight variants (`StepAudioEditX`, `MagpieMultilingual`, `Kokoro`, `VoxtralTts`)
  - Google Translate only exposes `Slow` and `Normal`
  - switching to Google Translate while current speed is `Fast` must coerce the saved speed to `Normal`
  - Kokoro settings expose: `voice` (string, e.g. `af_heart`), `speed` (0.5–2.0), `lang` (optional BCP-47), `num_threads` (1–8). No API key, no base URL — all inference is local.
  - The four libtorch-shim providers (Fish-Speech, Step Audio EditX, Magpie, Voxtral) install via `Settings → Downloaded Tools` — the "Open-weights TTS (libtorch)" card surfaces one row per model with the standard download/delete/size affordances used by Parakeet and Qwen3.
  - Gemini / Edge / Google preview interrupts current speech; preview text comes from the active UI locale bundle's `tts_preview_texts`.
  - Edge voice list is loaded from the live Edge catalog endpoint and cached locally.

## Failure And Recovery
- Missing Gemini key blocks Gemini TTS requests.
- Missing Cerebras key is irrelevant to TTS.
- Provider failure must end only the affected request and leave the runtime reusable for later requests.
- Interrupting a realtime request must not force the coordinator to forget unread committed text.
- Mobile keeps Android-specific audio output plumbing, but TTS must stay non-capturable by the app's own playback-capture path.

## Fixtures
- Shared fixtures: [parity-fixtures/tts-runtime/queue-semantics.json](../../parity-fixtures/tts-runtime/queue-semantics.json)
- Android unit tests must at minimum cover realtime skip-to-last-sentence, realtime replay-after-interrupt, and Google speed coercion.

## Deviations
- Android uses `AudioTrack` instead of Windows WASAPI for playback output.
- Android uses system language identification APIs plus script fallbacks instead of `whatlang`, but keeps the same ISO-639-3 / ISO-639-1 matching semantics at the settings and provider layers.
- Open-weights leaderboard TTS providers are not yet wired on Android. Kokoro runs on Windows via sherpa-onnx; the Android port of that pipeline is tracked as a follow-up. Selecting any leaderboard method on Android emits a clear "offline pipeline not yet available on Android" error from `AndroidTtsRuntimeService.runWorkerLoop` — the request channel still drains so the player stays usable.

## Open-Weights Catalog
- Authoritative entries live in [catalog/model_catalog.json](../../catalog/model_catalog.json) under `tts_open_models`.
- Each entry carries: `id`, `method` (matches `TtsMethod` variant name), `label`, `elo`, `runtime` (`sherpa-onnx` or `deferred`), plus either install metadata (`install_dir`, `download_primary`, `download_fallback_modelscope`, `required_files`, `required_archives`, `approx_size_mb`) or a `deferred_reason` explaining the upstream blocker.
- ModelScope mirrors are recorded as the second host because Hugging Face repositories occasionally gate or rate-limit by region; the Rust downloader tries HF first and falls back per file.
- Adding a new offline TTS provider follows: catalog entry → `TtsMethod` variant → `<Provider>Settings` struct (offline-only fields, no API keys) → asset module under `src/api/realtime_audio/<provider>_assets.rs` (with HF+ModelScope fallback) → worker file under `src/api/tts/worker/worker_<provider>.rs` → dispatch arm in `worker/mod.rs` → Windows settings panel in `gui/settings_ui/global/tts_settings.rs` → downloaded_tools card in `gui/settings_ui/global/downloaded_tools/model_sections.rs` → Android `MobileTtsMethod` variant → matching mobile settings struct → dispatch in `AndroidTtsRuntimeService` (initially emits "not yet available on Android" until the Android pipeline lands) → exhaustive match updates in `MobileShellDecor.kt` and `GlobalTtsSettingsDialogContent.kt`.
