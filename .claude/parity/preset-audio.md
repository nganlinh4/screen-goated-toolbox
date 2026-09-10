# Preset Audio Parity

## Canonical Source
- Windows audio preset defaults: [src/config/preset/defaults/audio.rs](../../src/config/preset/defaults/audio.rs)
- Windows audio preset hotkey/runtime entry: [src/hotkey/processor.rs](../../src/hotkey/processor.rs)
- Windows recording overlay shell: [src/overlay/recording/mod.rs](../../src/overlay/recording/mod.rs)
- Windows recording window/messages: [src/overlay/recording/window.rs](../../src/overlay/recording/window.rs), [src/overlay/recording/messages.rs](../../src/overlay/recording/messages.rs)
- Windows recording WebView template: [src/overlay/recording/ui.rs](../../src/overlay/recording/ui.rs)
- Windows record-then-process runtime: [src/api/audio/recording.rs](../../src/api/audio/recording.rs)
- Windows audio provider routing: [src/api/audio/transcription.rs](../../src/api/audio/transcription.rs), [src/api/audio/gemini_live.rs](../../src/api/audio/gemini_live.rs)
- Windows audio result/media templates: [src/overlay/process/pipeline.rs](../../src/overlay/process/pipeline.rs), [src/overlay/process/chain/templates.rs](../../src/overlay/process/chain/templates.rs)
- Windows automatic insertion owner: [streaming_paste.rs](../../src/overlay/utils/streaming_paste.rs), [policy.rs](../../src/overlay/utils/streaming_paste/policy.rs), [editor.rs](../../src/overlay/utils/streaming_paste/editor.rs)
- Android ownership reducer and platform shim: [ProvisionalPasteSession.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/ProvisionalPasteSession.kt), [AccessibilityProvisionalPasteTarget.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/AccessibilityProvisionalPasteTarget.kt)

## Behavior Contract
- Android audio presets launch from the bubble runtime, not from the main inspector screen.
- The shared text-input `mic` button and result-canvas `mic` action both launch the canonical `preset_transcribe` preset, matching Windows.
- Record-then-process audio presets use a dedicated recording session with Windows-style toggle semantics:
  - first launch starts capture
  - launching the same preset again while recording stops and submits
  - launching it again while processing aborts/closes
- Record sessions respect the Windows RMS/auto-stop thresholds:
  - warmup threshold `0.001`
  - speech threshold `0.015`
  - silence cutoff `800ms`
  - minimum speech window `2000ms`
- Built-in mic presets intended for one-shot speech capture keep `auto_stop_recording` aligned with the Windows defaults. This includes `preset_transcribe`, `preset_fix_pronunciation`, `preset_transcribe_retranslate`, `preset_quicker_foreigner_reply`, `preset_quick_ai_question`, `preset_voice_search`, and `preset_quick_record`.
- Android recording UI uses the generated Windows recording WebView template from `src/overlay/recording/ui.rs`; Android-only code is limited to the bridge prelude, touch-drag shim, and runtime token substitution.
- Audio-only input-adapter presets such as `preset_quick_record` and `preset_record_device` open the Windows-style audio-player result document rather than a text placeholder.
- Audio result/media documents stay under the normal result-window runtime and preserve the Windows media markers and raw-html bridge contract.
- The canonical `preset_transcribe` audio block should keep a generic transcription prompt in the default graph so alternate supported audio models inherit an instruction even though Whisper remains the default model.
- `gemini-live-audio` and `parakeet-local` stream partial transcript updates during capture and hand the final transcript into the first Android `AUDIO` block without forcing a second full transcription pass.
- When a streamed audio preset has `autoPaste = true`, Android incrementally injects transcript deltas into the currently focused editable target during capture and suppresses the final preset-level auto-paste to avoid double insertion.
- `autoPaste` is the sole automatic insertion switch, independent of preset name. Non-streaming models retain completed-result paste. Append-only streams append deltas; revisable streams can replace a session-owned provisional tail only in a verified editable destination. This policy does not change Live Translate's owned preview surface.
- Provisional replacement requires a bound focused non-password target, a collapsed caret, and exact expected surrounding text. Before and after every mutation, verify identity, focus, text and caret. Never erase by an unverified backspace count, force focus, overwrite a user selection, or use the clipboard. Finalized text replaces the current provisional tail once and becomes immutable. A frame containing both interim and final consumes the final only. Unsupported replacement capabilities use committed-only insertion if a safe append target exists; an unidentifiable or protected target receives no automatic output.
- Ownership loss or uncertain insertion suspends all further writes for that session, including final fallback; replaying a full result could duplicate or corrupt existing text. Pending interim updates may coalesce, but final segments retain order. Streaming control characters become spaces; no streaming output injects Enter or execution controls. Normal stop drains authoritative finals, then removes only a still-verifiably-owned provisional remainder. Abort discards queued output and attempts only verified provisional cleanup. Stale generations cannot write into a newer session. History/result handoff must not paste streamed output again.
- Dedicated live-transcription presets retain the shared hybrid speech-boundary lifecycle: after a real speech turn and the shared silence boundary, send one `audioStreamEnd` without stopping capture or closing the socket, then rearm on the next speech. This is separate from provisional destination rendering.
- Pending delivery is bounded to 128 events / 128 KiB UTF-8. Editable snapshots are bounded to 262,144 UTF-16 units and one insertion to 16,384 UTF-16 units. Exceeding a safety bound never triggers an unverified full-result paste.
- Realtime audio is not a preset operation. Live Translate is launched only through its official mini-app entry, while audio presets remain record-then-process workflows.
- Device-audio presets use inline permission/MediaProjection handoff through the app, then resume the pending preset launch automatically.
- The bubble host must temporarily promote itself into `microphone` or `mediaProjection` foreground-service mode before starting preset audio capture, then restore normal bubble mode after stop/cancel/failure.

## Failure And Recovery
- Missing `RECORD_AUDIO` permission or missing MediaProjection consent must route through the app permission flow instead of leaving the preset on a placeholder toast.
- Missing provider keys should surface as execution errors on the preset result path rather than crashing the bubble runtime.
- Capture failures must retain the concrete error detail for logging instead of collapsing everything into a generic preset toast.
- Preset auto-speak uses the dedicated auto-speak TTS consumer and retries one first-use playback failure before surfacing a user-visible error.
- Gemma 4 is not currently an audio-input model family in this app; do not expose it in audio transcription pickers or routing paths.

## Fixtures
- Shared fixture: [parity-fixtures/preset-system/audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)
- Final-only streaming typing examples: [parity-fixtures/preset-system/streaming-typing.json](../../parity-fixtures/preset-system/streaming-typing.json)
- Owned provisional insertion examples: [parity-fixtures/preset-system/provisional-paste.json](../../parity-fixtures/preset-system/provisional-paste.json)

## Deviations
- Windows destination ownership is shorter-lived than the recording session. Focus changes or user edits abandon the old provisional tail without cleanup. The interrupted segment's remaining hypotheses/final are discarded for insertion; after its final boundary and 250 ms of stable foreground/input, automatic binding resumes without restarting capture. No transcript is replayed across destinations. Abort, stop, expired workers, and superseded sessions never rebind. Android currently retains session-long destination ownership.
- Windows supports best-effort keyboard provisional revisions when accessibility cannot describe the destination. Missing detection does not block streaming; native foreground/focus identity and user-input epoch remain bound to the invocation. Corrections preserve the common Unicode-scalar prefix and backspace the changed suffix before inserting its replacement in one tagged input batch. This assumes scalar backspace behavior; application transformations, grapheme deletion, and unobservable caret changes cannot be verified. It never retries uncertain delivery or replays the complete result. Explicit password/disabled controls remain excluded. Android retains its Accessibility-based insertion capability requirement.
- Accessibility selection and input are separate platform operations, not an atomic compare-and-set. Both platforms validate immediately before and after effects and suspend on uncertainty. Windows uses a dedicated bounded worker; Android serializes native Accessibility actions on its service's Main dispatcher. A Windows worker that misses its finish deadline or is superseded cannot perform later cleanup, so an unverified provisional remainder may remain visible rather than risking user text.
- Android does not currently implement the Windows local `parakeet-local` preset audio runtime. Until a real mobile Parakeet runtime is wired, presets that depend on `parakeet-local` must be marked unsupported with an explicit provider/runtime placeholder instead of launching and failing at execution time.
