# TTS Runtime Parity

## Canonical Source

- Windows manager/workers/player: [tts](../../src/api/tts)
- Android manager/providers/player: [tts](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts)
- Shared Android Gemini Live transport: [GeminiLiveReadySession.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveReadySession.kt), [GeminiLiveTransport.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveTransport.kt)
- Windows realtime consumer: [realtime_webview](../../src/overlay/realtime_webview)
- Windows config/catalog: [tts.rs](../../src/config/types/tts.rs), [model_catalog.json](../../catalog/model_catalog.json)
- Shared queue contract: [queue-semantics.json](../../parity-fixtures/tts-runtime/queue-semantics.json)

## Cross-Platform Contract

- Keep a global request manager, provider workers, one playback lane, and feature-local producer queues.
- Every request has a unique ID. Normal and realtime requests append.
- Interrupt increments generation, clears queued work, cuts current playback, and starts the replacement. Stop increments generation and clears all work.
- Realtime Read/TTS enqueues only newly committed translation text.
- Enabling it on existing history begins at the last prior sentence boundary, not the start of the transcript.
- Hiding the translation surface stops speech. Disabling resets queued/spoken offsets.
- Automatic speed follows unread realtime backlog, not total transcript length.
- Provider failure ends only that request; later requests must remain usable.
- An interrupt must not make the producer forget committed text that still needs replay.

## Provider Boundary

- Gemini Live, Edge, and Google Translate exist on both platforms with platform-native playback.
- Gemini Live requests use the shared setup-gated transport on both platforms. Application
  content cannot be sent until a structural top-level `setupComplete` acknowledgement arrives.
- A warm Gemini Live entry owns only an opened connection. Setup remains request-specific, and
  a retryable stale warm connection may be replaced by one fresh connection only before any
  request content has been accepted.
- Edge and Google Translate keep their provider-specific WebSocket/HTTP transports; Gemini Live
  lifecycle policy must not leak into those providers.
- Edge prosody is encoded in SSML and must not be applied again by the shared player.
- Android exposes only providers with a real Android runtime and normalizes stale Windows-only values to a supported method.
- Windows open-weight providers and installation details belong to `catalog/model_catalog.json` plus the matching `native/<runtime>/README.md`, not this parity spec.

## Deviations

- Windows plays through WASAPI; Android uses `AudioTrack` and prevents self-capture by playback capture.
- Language identification implementations differ, but settings/provider matching preserves the same ISO language semantics.
- Windows-only local providers remain absent from Android until a verified mobile runtime exists.

Tests must cover queue order, interrupt generation, realtime skip/replay, and Google speed coercion against the shared fixture.
