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
- Windows realtime overlay/runtime: [src/overlay/realtime_webview/manager.rs](../../src/overlay/realtime_webview/manager.rs), [src/overlay/realtime_egui.rs](../../src/overlay/realtime_egui.rs)

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
- Realtime audio presets use the existing Android live-translate service through a transient preset-backed session config. The user’s saved launcher config must be restored after the session ends.
- Device-audio presets use inline permission/MediaProjection handoff through the app, then resume the pending preset launch automatically.
- The bubble host must temporarily promote itself into `microphone` or `mediaProjection` foreground-service mode before starting preset audio capture, then restore normal bubble mode after stop/cancel/failure.

## Failure And Recovery
- Missing `RECORD_AUDIO` permission or missing MediaProjection consent must route through the app permission flow instead of leaving the preset on a placeholder toast.
- Missing provider keys should surface as execution errors on the preset result path rather than crashing the bubble runtime.
- Realtime preset stop must clear the transient preset override and the tracked active realtime preset id.
- Capture failures must retain the concrete error detail for logging instead of collapsing everything into a generic preset toast.
- Preset auto-speak uses the dedicated auto-speak TTS consumer and retries one first-use playback failure before surfacing a user-visible error.
- Gemma 4 is not currently an audio-input model family in this app; do not expose it in audio transcription pickers or routing paths.

## Fixtures
- Shared fixture: [parity-fixtures/preset-system/audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)

## Deviations
- Android does not currently implement the Windows local `parakeet-local` preset audio runtime. Until a real mobile Parakeet runtime is wired, presets that depend on `parakeet-local` must be marked unsupported with an explicit provider/runtime placeholder instead of launching and failing at execution time.
