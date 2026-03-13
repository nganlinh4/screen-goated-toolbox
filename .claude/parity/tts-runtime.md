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
  - Google Translate TTS uses HTTP MP3 -> decode -> shared player
- Global settings behavior:
  - methods are `Gemini Live`, `Edge TTS`, and `Google Translate`
  - Google Translate only exposes `Slow` and `Normal`
  - switching to Google Translate while current speed is `Fast` must coerce the saved speed to `Normal`
  - Gemini voice preview interrupts current speech
  - Edge preview interrupts current speech
  - preview text comes from the active UI locale bundle's `tts_preview_texts`, not from mobile-only hard-coded demo sentences
  - Edge voice list is loaded from the live Edge catalog endpoint and cached locally

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
