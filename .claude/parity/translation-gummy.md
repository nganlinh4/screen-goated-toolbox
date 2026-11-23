# Translation Gummy Parity

## Canonical Source

- Windows surface/runtime: [translation_gummy](../../src/overlay/translation_gummy)
- Windows settings: [translation_gummy.rs](../../src/config/types/translation_gummy.rs)
- Windows feature volume state: [volume.rs](../../src/overlay/translation_gummy/volume.rs)
- Android feature: [translationgummy](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy)
- Shared Android Live transport/parser: [GeminiLiveReadySession.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveReadySession.kt), [GeminiLiveProtocol.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveProtocol.kt)
- Shared fixtures: [translation-gummy](../../parity-fixtures/translation-gummy)

## Contract
- Local speech activity uses the shared noise-relative policy in `parity-fixtures/preset-system/microphone-activity.json`; `speechRms` is the maximum threshold, not a fixed quiet-speech gate.

- Two profiles each require a language and may add accent/tone.
- Detect the spoken profile and output only the translation in the other profile.
- Use the globally selected Gemini Live model and voice, independent of the active global TTS method.
- Build setup per selected model. Gemini 2.5 uses `thinkingBudget`; Gemini 3.1 uses `thinkingLevel`. Do not force one family's field onto the other.
- Enable input/output transcription, interruption on new activity, context compression, and the VAD/setup values locked by `vad-contract.json`.
- Process every content part in a server event; Gemini 3.1 may combine audio and transcript parts.
- Capture and media sending begin only after structural `setupComplete`; setup errors win over acknowledgement. Content arriving during setup, including in the acknowledgement frame, is delivered once to the active session in arrival order. Setup is limited to 64 frames and 1 MiB of UTF-8 payload, including the first acknowledgement; overflow fails the attempt. Android coalesces subsequent pure duplicate acknowledgements. Every active audio/stream-end send rejection terminates that transport attempt.
- Normal local mic input is forwarded continuously; local noise-relative activity adds `audioStreamEnd` after end-of-speech but cannot discard quiet input. Playback/barge-in suppression remains a separate echo-safety policy, with bounded preroll on Android.
- Valid saved config auto-starts. Apply persists a valid draft, preserves transcript history, inserts the session separator, and restarts.
- Start/Stop controls the last applied config. Reconnects preserve transcript history and keep the session visibly running while a retry is pending.
- Both platforms use the shared Gemini Live lifecycle and its default backoff, with at most six consecutive reconnects. Meaningful server activity resets the attempt count; setup acknowledgement alone does not. Retryable transport/server failures and normal or going-away socket closes reconnect; non-retryable server failures, policy closes, and exhausted attempts leave a visible terminal error. A user can explicitly start again after correcting the problem.
- Stop, Apply, window/service teardown, and a replacement start retire the previous owner. Retired attempts cannot publish connection or transcript content, schedule retries, retain audio ownership, or affect the replacement. A delayed language result may enrich a finalized historical row only if its identity and text still match. An audio-device open or network send that passed its final stop check can start or finish after retirement; an opened device must be discarded as soon as it returns. Stop during connection, setup, or backoff must not produce a later Ready state. Cancellation and terminal cleanup release owned capture/playback resources; terminal errors remain visible.
- Partial transcript rows update in place; final rows resolve to the matching language side; the visualizer reflects connection/readiness state.
- Every delivered transcript snapshot is authoritative for every retained row. Earlier input rows may grow after an output row has appeared. Rendering must compare complete row values, preserve keyed row identity, and display text changes even when row count, last-row length, and language values are unchanged. Desktop history/delete labels follow the current locale, including existing controls. Android derives transcript groups from the current immutable list.
- Persist finalized transcript rows, retaining role, text, and detected language. Windows serializes saves including snapshot selection, writes a synchronized temporary sibling, and atomically replaces the history file. A failed save preserves the previous complete file, reports a diagnostic without transcript content, and permits a later save to retry. Existing history and legacy history formats remain readable. Android retains its encrypted preference storage and serialized persistence worker; filesystem replacement behavior is a Windows platform contract.
- Audio meter samples are ephemeral, independent of settings and transcript publication. A meter-only update must not normalize settings, serialize history, persist transcripts, or rerender transcript controls. Values are finite and clamped to `0..1`; stopped sessions show zero. Only the current session owner can publish.
- Meter UI delivery is limited to one latest-value update per 50ms. Bursts coalesce; a retired owner or stale window wake cannot replace a current pending update. Full connection/settings/transcript snapshots retain their existing behavior and reset stopped presentation immediately. Windows uses a small numeric event consumed by the existing animation loop; Android collects a separate sampled flow only at the waveform. Native/device rendering acceptance is separate from reducer/JVM/frontend tests.
- Volume is feature-local (`0..100`, default 100, 5-point UI steps). Mute remembers the latest nonzero slider value,including after the slider reaches zero. Volume changes playback only and do not restart the session or change another feature's volume. A settings refresh or reconnect preserves the current feature volume.
- Current volume and the remembered unmute target belong to feature state together. Recreating controls while muted must retain that target for the lifetime of the feature owner. Mute toggles act on the owner's current state;the view projects its coherent snapshot. Repeated snapshots with unchanged volume must leave the control's content untouched.
- The onboarding guide appears until dismissal persists `guide_seen = true`. Applying an older draft must preserve that independent dismissal flag and the separately managed hotkeys.

## Entry and Deviation

- Android: second Apps carousel card, native Compose UI, foreground service, no feature hotkey.
- Windows: footer launcher and optional hotkey, Wry surface, session stops when the window closes.
- Windows loads a legacy single `hotkey` into `hotkeys` before shortcut registration or any settings save. Binding identity is code plus modifiers. A legacy binding already in the list is not inserted again; a distinct legacy binding precedes the existing list without reordering it. Repeated migration is idempotent. Saved configuration contains only the current list, including after an unrelated setting changes without opening Gummy. Blank-language defaults are restored before normalizing settings. Android does not register Windows global shortcuts.
- Android keeps volume settings in the touch-friendly TTS modal; Windows uses its desktop header control.

## Fixtures

- [prompt-contract.json](../../parity-fixtures/translation-gummy/prompt-contract.json)
- [socket-protocol.json](../../parity-fixtures/translation-gummy/socket-protocol.json)
- [vad-contract.json](../../parity-fixtures/translation-gummy/vad-contract.json)
- [onboarding-contract.json](../../parity-fixtures/translation-gummy/onboarding-contract.json)
- [volume-control.json](../../parity-fixtures/translation-gummy/volume-control.json)
- [state-contract.json](../../parity-fixtures/translation-gummy/state-contract.json)
- [reconnect-contract.json](../../parity-fixtures/translation-gummy/reconnect-contract.json)
- [meter-delivery.json](../../parity-fixtures/translation-gummy/meter-delivery.json)
- [transcript-rendering.json](../../parity-fixtures/translation-gummy/transcript-rendering.json)
- [transcript-persistence.json](../../parity-fixtures/translation-gummy/transcript-persistence.json)
- [hotkey-migration.json](../../parity-fixtures/translation-gummy/hotkey-migration.json)

## Current Implementation Debt

Windows, Android, and `vad-contract.json` derive thinking setup from exact endpoint metadata: Gemini 2.5 uses `thinkingBudget`; Gemini 3.1 uses `thinkingLevel`.

Android's active-session callback queue has the bounded overflow behavior in [Gemini Live Session Parity](gemini-live-session.md).
