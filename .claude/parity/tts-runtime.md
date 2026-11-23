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
- Realtime request registration, playback completion, stop, and offset reset are
  serialized as one producer state transition. Completion cannot race ahead of
  request registration, and a retired request cannot advance a later run.
- Each realtime producer keeps at most one request outstanding. New committed
  text waits behind it; a completed or partially played request advances its
  boundary before the next request is admitted. A failed or interrupted
  request cannot be skipped by a later completion. A transcript reset retires
  the prior producer generation without interrupting an unrelated consumer.
- Requests discarded by interruption produce a terminal interruption event;
  accepted playback events are delivered in order without silent overflow loss.
- Android reserves bounded completion capacity before accepting a request or
  interrupting existing speech. Each subscription receives only its selected
  consumer/owner events; bounded capacity is reserved per subscriber and per
  consumer so a stalled subscriber cannot exhaust unrelated owners' capacity.
  A stalled owner cannot silently drop a completion. Capacity rejection is visible and has
  no playback effect; closing a subscriber releases its pending acknowledgments.
- A realtime producer is disposed with its owning service. Interruption before
  audio admission permits retry; after any audio is accepted the segment is
  partial and must not be replayed from its beginning. A replacement speech
  request resumes unread final text behind the replacement, regardless of its
  consumer or the order of terminal events. A standalone stop does not cause
  automatic retry of previously committed text, including after partially
  played segments and while admission is at capacity. New text committed after
  a global stop starts a fresh producer epoch; retries do not loop on provider
  failure.
- If a producer is stopped without resetting its transcript, an in-flight
  segment is treated as potentially heard and is not replayed on resume.
- A transcript replacement starts a new producer generation; an append after
  another consumer stops speech speaks only the newly committed suffix.
- Preset speech remains bound to its session through dispatch, while admission
  can wait without holding the session ownership lock.

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
- Edge synthesis has a bounded setup and network session through `turn.end`, a bounded encoded-audio
  buffer, and request-owned cancellation. Only a nonempty response terminated by
  the provider's `turn.end` is eligible for playback or export; timeout, close,
  malformed transport frames, reported decode errors, and overflow fail the
  request without playing partial audio. Decoder APIs may not detect a final
  truncated MP3 frame; this transport contract does not establish MP3 integrity.
  MP3 decoding and playback follow the network session and do not share its
  deadline or encoded-byte bound.
  Cancellation may wait for an in-flight bounded connection attempt; a cancelled
  DNS resolver retains its capacity slot until it settles. The shared cases are
  in [edge-transport.json](../../parity-fixtures/tts-runtime/edge-transport.json).
- Android exposes only providers with a real Android runtime and normalizes stale Windows-only values to a supported method.
- Windows-local providers and their installation details belong to the Windows component registry (`docs/COMPONENT_DELIVERY.md`) and `docs/OFFLINE-ENGINES.md`, not this parity spec.

## Deviations

- Windows plays through WASAPI; Android uses `AudioTrack` and prevents self-capture by playback capture.
- Language identification implementations differ, but settings/provider matching preserves the same ISO language semantics.
- Windows-only local providers remain absent from Android until a verified mobile runtime exists.

## Settings voice preview

- A per-language preview snapshots the clicked row's current language and voice,
  including edits that have not yet been saved. It explicitly selects that voice;
  language detection and duplicate rows cannot redirect the request.
- Samples use the selected language where a shared sample is available. Otherwise
  the shared English fallback is used, while voice selection remains explicit.
- Starting another settings preview cancels only the prior settings preview.
  Unrelated queued or playing speech remains owned by its original caller.
- [voice-preview.json](../../parity-fixtures/tts-runtime/voice-preview.json)
  defines representative language, duplicate-row, and fallback cases. Both
  platforms apply this contract to Edge rows.

Tests must cover queue order, interrupt generation, realtime skip/replay, and Google speed coercion against the shared fixture.
