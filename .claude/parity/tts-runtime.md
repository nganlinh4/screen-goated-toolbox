# TTS Runtime Parity

## Canonical Source
- Windows runtime entrypoint: [src/api/tts/mod.rs](../../src/api/tts/mod.rs)
- Windows manager queue + interrupt model: [src/api/tts/manager.rs](../../src/api/tts/manager.rs)
- Windows provider workers: [src/api/tts/worker/mod.rs](../../src/api/tts/worker/mod.rs)
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
  - Step Audio EditX and NVIDIA Magpie-Multilingual run through managed persistent Python sidecars. Each runtime is a downloadable app-owned bundle with Python/PyTorch and model-specific source code; customer machines must not run `pip`.
  - Mistral Voxtral 4B still uses the shared libtorch-shim placeholder pattern until a real runtime is selected.
  - All open-weights workers must emit PCM16 mono at 24 kHz (the same rail Gemini and Edge ride). Higher source rates are resampled in `super::resample_audio`.
- Asset download contract (Kokoro is the canonical example; future offline TTS providers must follow it):
  - Files land in `dirs::data_dir()/screen-goated-toolbox/models/<id>/` (e.g. `models/kokoro_v1/`).
  - Primary host is Hugging Face; the worker must try a **ModelScope mirror as a per-file fallback** so gated/region-restricted repos still install on first try.
  - phonemizer / aux data shipped as `.tar.bz2` is extracted via the OS-bundled `tar` binary; the archive is removed after extraction.
  - `is_<model>_downloaded()` checks one mandatory file per category (weights, voices/embeddings, tokenizer, phonemizer marker) rather than a single sentinel — partial installs from cancelled downloads must read as "missing".
- Global settings behavior:
  - Windows methods are `Gemini Live`, `Edge TTS`, `Google Translate`, plus the visible Windows open-weight variants (`StepAudioEditX`, `MagpieMultilingual`, `Kokoro`, `Supertonic`, `VieneuTts`)
  - Android exposes only methods with a real Android runtime: `Gemini Live`, `Edge TTS`, and `Google Translate`
  - Android must normalize stale persisted/deep-linked open-weight method values back to `Gemini Live` instead of surfacing an unusable selector entry
  - `VoxtralTts` is a legacy/deferred config value and is not exposed in the Windows selector; Windows coerces it back to `VieneuTts`
  - Google Translate only exposes `Slow` and `Normal`
  - switching to Google Translate while current speed is `Fast` must coerce the saved speed to `Normal`
  - Kokoro settings expose: `voice` (string, e.g. `af_heart`), `speed` (0.5–2.0), `lang` (optional BCP-47), `num_threads` (1–8). No API key, no base URL — all inference is local.
  - Open-weight providers install via `Settings → Downloaded Tools`; each model card has separate model-weight and runtime rows with the standard download/delete/size affordances used by Parakeet and Qwen3.
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
- Open-weights leaderboard TTS providers are feature-excluded on Android until a real mobile runtime exists. Kokoro and Supertonic run on Windows via sherpa-onnx, while VieNeu/Step/Magpie use managed local runtimes that are not viable to expose as normal Android phone/tablet options. Android may keep enum/settings fields for backward-compatible deserialization, but the selector must not show those methods and stale values must normalize to `Gemini Live`.

## Open-Weights Catalog
- Authoritative entries live in [catalog/model_catalog.json](../../catalog/model_catalog.json) under `tts_open_models`.
- Each entry carries: `id`, `method` (matches `TtsMethod` variant name), `label`, `elo`, `runtime` (`sherpa-onnx`, `managed-sidecar`, or `deferred`), plus either install metadata (`install_dir`, `download_primary`, `download_fallback_modelscope`, `required_files`, `required_archives`, `approx_size_mb`) or a `deferred_reason` explaining the upstream blocker.
- ModelScope mirrors are recorded as the second host because Hugging Face repositories occasionally gate or rate-limit by region; the Rust downloader tries HF first and falls back per file.
- Adding a new offline TTS provider follows: catalog entry → `TtsMethod` variant → `<Provider>Settings` struct (offline-only fields, no API keys) → asset module under `src/api/realtime_audio/<provider>_assets.rs` (with HF+ModelScope fallback) → worker file under `src/api/tts/worker/worker_<provider>.rs` → dispatch arm in `worker/mod.rs` → Windows settings panel in `gui/settings_ui/global/tts_settings.rs` → downloaded_tools card in `gui/settings_ui/global/downloaded_tools/model_sections.rs`. Android only adds a selector entry after a real mobile runtime has been implemented and verified on target devices.
