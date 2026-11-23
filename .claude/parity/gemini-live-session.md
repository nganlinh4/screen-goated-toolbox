# Gemini Live Session Parity

- Native Live endpoints are selectable through the shared catalog for text,
  vision, input transcription, custom audio, and TTS. Gemini 3.8 Live uses the
  existing modality routes with its exact endpoint profile, omitting thinking
  configuration. Adding a choice does not change a feature's saved/default model.

## Canonical Source

- Windows structural frame decoder: [src/api/gemini_live/server_frame.rs](../../src/api/gemini_live/server_frame.rs)
- Windows setup-gated transport: [src/api/gemini_live/ready_session.rs](../../src/api/gemini_live/ready_session.rs)
- Windows finite request lifecycle: [src/api/gemini_live/worker.rs](../../src/api/gemini_live/worker.rs)
- Windows continuous reconnect/rotation policy: [src/api/realtime_audio/s2s/live/continuous.rs](../../src/api/realtime_audio/s2s/live/continuous.rs)
- Windows transcription recovery: [src/api/realtime_audio/transcription/main_loop.rs](../../src/api/realtime_audio/transcription/main_loop.rs)
- Windows agent protocol and reconnect safety: [src/overlay/computer_control/protocol.rs](../../src/overlay/computer_control/protocol.rs), [src/overlay/computer_control/runtime](../../src/overlay/computer_control/runtime)
- Android structural frame decoder: [GeminiLiveProtocol.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveProtocol.kt)
- Android setup-gated transport: [GeminiLiveReadySession.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveReadySession.kt)

## Behavior Contract
- Continuous transcription treats a failed audio write as uncertain delivery:
  do not replay the attempted frame, retain later unattempted PCM in order, and
  present a possible audio-gap notice without modifying transcript state.
  The replay queue retains at most the newest 60 seconds; overflow drops the
  oldest samples and reports the gap. Windows capture may independently hold up
  to 60 seconds while a connection attempt is blocked; merging applies the same
  replay limit. Recoverable disconnects retry with the shared backoff policy
  for at most six attempts; cancellation, source/model replacement, and fatal
  setup errors stop retries. Exhaustion is visible rather than a normal finish.
  Both platforms validate the retry schedule from
  [transcription-send-recovery.json](../../parity-fixtures/gemini-live-session/transcription-send-recovery.json);
  its queue cases also cover Android's chunked-send adapter.
- A failed resumed setup abandons that handle before fallback, so later
  reconnects do not repeatedly submit a rejected handle. A frame containing
  transcription and `goAway` delivers its transcription before reconnecting.
- Setup acknowledgement is released only after the remaining observations from
  its frame have been queued. Every connection replacement waits for any admitted
  frame callback to finish and delivers its queued transcript and resumption
  observations before reconnecting; callbacks admitted after retirement are
  rejected. Capture ending during setup or retry delay stops further retries.
- Dedicated live transcription uses `SMART` in every preset and Live Translate setup, including reconnects. Interim hypotheses remain revisable; this mode does not imply committed-prefix stability. Batch transcription is a separate contract.
- Local transcription speech boundaries use the shared noise-relative activity contract in `parity-fixtures/preset-system/microphone-activity.json`; PCM amplitude is unchanged.

- Windows and Android implement the shared fixture as a pure, clock-injected reducer. JSON decoding, sockets, sleeping, audio playback, and feature transcript state stay outside it.
- The reducer input is `(state, event, monotonicNowMs, policy)` and its output is `(nextState, orderedEffects)`. An omitted field in `expectState` is not asserted; `expectEffects` is always an exact ordered list.
- Monotonic times, durations, and work/input counters use the shared nonnegative signed-64-bit domain; reconnect attempts use the nonnegative signed-32-bit domain. Addition saturates at the matching maximum; Rust normalizes wider unsigned inputs to those boundaries, and Kotlin rejects negative inputs.
- Transport adapters execute effects and feed observations back as events. They must not reinterpret completion, interruption, retry, rotation, or cancellation policy.
- Session phases are `idle`, `connecting`, `awaitingSetup`, `active`, `backingOff`, `completed`, `cancelled`, and `failed`. The last three are terminal and absorb every later event.
- Every socket observation carries a positive `generation`. Starting or abandoning a connection advances the accepted generation. Events from any other generation produce no state change, effects, activity refresh, or backoff reset.
- Normalized events are:
  - `start`, `socketOpened`, `frame`, `transportFailure`, `inputSent`, `inputActivity`, `workState`, `tick`, and `cancel`.
  - A `frame` preserves all structural fields from one server frame together: content count, `setupComplete`, `turnComplete`, `generationComplete`, `interrupted`, `goAwayTimeLeftMs`, tool-call IDs, tool-cancellation IDs, and classified error.
  - Local `inputSent`, `inputActivity`, and `workState` events contain only structural counters/booleans. They never infer intent from language.
- Ordered effects are transport-neutral commands such as `openSocket`, `sendSetup`, `deliverContent`, `finalizeResponse`, `finalizeGeneration`, `finalizeTurn`, `stopPlayback`, `discardQueuedOutput`, `dispatchTools`, `cancelTools`, `closeSocket`, `scheduleReconnect`, `reportFailure`, and `cancelSession`.
- All content in a frame is delivered before a completion effect from that same frame. All audio/content parts remain available to the feature adapter. If a continuous frame carries both completion fields, `finalizeTurn` is emitted before `finalizeGeneration`; neither boundary is dropped.
- Completion semantics remain distinct:
  - A finite endpoint with catalog `live_completion: interaction-idle` completes
    only on structural `serverContent.interactionStatus: IDLE`. Turn/generation
    boundaries, silence after partial content, and socket closure cannot finalize
    its response. Its hard response deadline remains bounded on both platforms.
    An explicit null `live_thinking` omits thinking configuration from setup.
  - A finite request completes on either `turnComplete` or `generationComplete` according to its policy.
  - A continuous session treats `generationComplete` as a generation boundary and `turnComplete` as a turn boundary; neither closes the socket.
  - An agent session never promotes `generationComplete` into `turnComplete`.
- `interrupted` stops current playback, discards queued model output, and finalizes only the interrupted generation. It is not user cancellation, transport failure, session completion, or tool cancellation.
- `toolCancellationIds` cancel only matching pending tool IDs. Unknown IDs are ignored. Interruption never cancels a tool implicitly.
- `goAway` records an absolute deadline derived from `monotonicNowMs + timeLeftMs`. It is neither an error nor completion. Reconnect waits for a safe gap (`userSpeaking=false`, no pending work, no buffered input), but the deadline forces reconnect if a safe gap never arrives.

## Failure And Recovery

- Android bounds unread active WebSocket payloads to 128 frames and 8 MiB of UTF-8 payload. An over-limit frame terminates the connection with an explicit non-retryable receive-buffer error and discards undelivered buffered frames, so a queued completion cannot hide the overflow. Frames already delivered remain ordered. Draining a frame releases its capacity. A timeout or cancellation while waiting leaves payloads queued for the next receive. A ready session admits only one receive at a time and rejects overlapping receives. Close and failure remain observable even when the buffer is full. The Android-specific adapter limit is covered by [android-event-buffer.json](../../parity-fixtures/gemini-live-session/android-event-buffer.json); Windows pulls frames synchronously and does not hold this callback queue.
- A non-retryable failure of the selected segmented S2S attempt terminates the session rather than replaying audio it may already have emitted. A failed hedge cannot abort a different successful winner; if no hedge wins, a non-retryable failure is reported instead of treating the segment as empty.
- Setup timeout begins when `sendSetup` is emitted. `setupComplete` must arrive on the current generation before input or catch-up replay is allowed.
- A socket close or a rejected setup write before acknowledgement immediately
  fails that setup and enters the normal retry policy; it does not wait for the
  setup timeout. A close after acknowledgement remains an active-session event.
- Setup gating consumes only the acknowledgement signal. Other structural observations carried before or in the same frame as `setupComplete` are buffered and delivered in wire order after activation.
- Finite response policy owns separate first-response, content-idle, and hard-response deadlines. First content clears the first-response deadline. A completion signal wins over deadlines in the same reducer call.
- Finite caller teardown retires its queued request and accepted socket without
  changing another request's cancellation flag. Cancellation is rechecked after
  blocking receives and reads,before publishing decoded output. Transport cleanup
  also runs when a callback fails or the response consumer is abandoned.
- Continuous server-idle recovery requires both the configured elapsed silence and the configured number of outbound chunks since the last server activity. A quiet user must not churn sockets.
- Native-audio transcription no-result recovery waits for a completed input-flush boundary after sufficient successfully sent active audio, then grants a full response grace period. An unfinished speech interval is not a failed response. Meaningful server content/completion/interruption clears this evidence; usage/setup acknowledgements do not. Every new connection starts with fresh evidence. The pure adapter guard is covered by [transcription-recovery.json](../../parity-fixtures/gemini-live-session/transcription-recovery.json).
- Reconnect catch-up preserves unsent input in wire order, returns to normal sending once drained on every transcription protocol, and continues dedicated speech-end observation while replaying.
- Retryable failures close/abandon the current generation and schedule exactly one reconnect. Server retryability is classified only from structural protocol `code`/`status` fields; transient statuses such as `UNAVAILABLE` and `RESOURCE_EXHAUSTED` use reconnect, while auth/configuration errors enter `failed` without retry.
- Socket opening tries resolved IPv4 and IPv6 addresses after a TCP failure rather than relying on the first DNS result. Windows bounds resolution and TCP attempts under one connection budget and interleaves address families; Android delegates address routing to its HTTP transport. The Windows adapter cases are in [transport-address-fallback.json](../../parity-fixtures/gemini-live-session/transport-address-fallback.json).
- Reconnect delay is fixture-owned and deterministic: `min(maxMs, baseMs * 2^min(attempt, exponentCap) + jitterMinMs + ((jitterSeed + attempt * jitterStep) % jitterSpan))`. The current attempt computes the delay, then the stored attempt increments.
- `setupComplete` alone does not reset backoff. The first meaningful active frame (content, completion, interruption, or tool event) resets it; error, `goAway`, usage-only, malformed, and unknown frames do not.
- Proactive rotation requires socket age, quiet input, quiet server, no pending work, no buffered input, and no active speech. Rotation uses the normal reconnect path and never creates two live generations.
- `cancel` is terminal in every nonterminal phase. It clears all deadlines, closes the accepted socket when present, prevents reconnect, and makes later callbacks stale/absorbed.
- Deadline comparison is inclusive (`nowMs >= deadlineMs`) and uses only a monotonic clock. When several reconnect causes become true together, precedence is `cancel`, fatal error, `goAway` deadline, setup/response deadline, server idle, then proactive rotation; only one reconnect or terminal effect sequence may be emitted.

## Fixtures

- Shared lifecycle fixture: [parity-fixtures/gemini-live-session/lifecycle.json](../../parity-fixtures/gemini-live-session/lifecycle.json)
- Rust and Android JVM tests must replay the same arrangements and cases. Fixture policy values are test inputs to the reducer, not constants to duplicate in adapters.
- Raw JSON-to-frame decoding remains covered separately by feature protocol fixtures; this fixture begins after structural decoding.
- Still frames for image presets follow [still-frame.json](../../parity-fixtures/gemini-live-session/still-frame.json): a frame whose long edge exceeds the budget is scaled down proportionally, and a smaller frame is sent at its own size, never enlarged or shrunk. Windows replays it in `src/api/gemini_live/websocket.rs`; Android uses `GeminiLiveStillFrameSize.kt` from its production bitmap encoder. Both send four frames with three 500 ms intervals, without delaying after the final frame.

## Windows Continuous S2S Adapter

- [src/api/realtime_audio/s2s/live/lifecycle_adapter.rs](../../src/api/realtime_audio/s2s/live/lifecycle_adapter.rs) is the transport adapter for continuous Live Translate. It owns the monotonic epoch, accepted generation, connected/ready typestate, and exact ordered execution of reducer effects.
- `openSocket`, `sendSetup`, `closeSocket`, and `scheduleReconnect` are executed only by that adapter. Feature output effects are handed to [drain.rs](../../src/api/realtime_audio/s2s/live/drain.rs) in reducer order.
- Captured audio remains in the bounded pending-input buffer until the reducer is active and `ReadyLiveSession` exists. A failed send does not dequeue its frame.
- `WorkState.bufferedInputCount` is the pending PCM sample count; `pendingWorkCount` is queued playback output; `userSpeaking` is the structurally measured current input frame state.
- Continuous generation and turn boundaries remain distinct reducer effects. They do not close the Live Translate socket.

## Deviations

- Default: none.
- An adapter is not conformant until it passes the shared fixture. Temporary migration gaps are implementation debt, not platform-specific behavior.
- Windows DNS/TCP/TLS/WebSocket opening remains synchronous to callers and observes cancellation after connection setup returns. DNS resolver workers are bounded, but the underlying operating-system lookup cannot be interrupted; setup and active polling already have short cancellation bounds.
