# Live Translate Parity

## Canonical Source
- Windows entry UI shell: [src/overlay/realtime_html.rs](../../src/overlay/realtime_html.rs)
- Windows realtime HTML styling: [src/overlay/html_components/css_main.rs](../../src/overlay/html_components/css_main.rs)
- Windows realtime HTML behavior: [src/overlay/html_components/js_main.rs](../../src/overlay/html_components/js_main.rs)
- Windows shared realtime state: [src/api/realtime_audio/state.rs](../../src/api/realtime_audio/state.rs)
- Windows transcription loop: [src/api/realtime_audio/transcription.rs](../../src/api/realtime_audio/transcription.rs)
- Windows translation loop: [src/api/realtime_audio/translation.rs](../../src/api/realtime_audio/translation.rs)
- Windows realtime model config: [src/model_config.rs](../../src/model_config.rs)
- Windows realtime overlay state: [src/overlay/realtime_webview/state.rs](../../src/overlay/realtime_webview/state.rs)
- Windows realtime overlay IPC + controls: [src/overlay/realtime_webview/webview.rs](../../src/overlay/realtime_webview/webview.rs)
- Windows app selection popup: [src/overlay/realtime_webview/app_selection.rs](../../src/overlay/realtime_webview/app_selection.rs)
- Windows text chunk UI behavior: [src/overlay/html_components/js_logic.rs](../../src/overlay/html_components/js_logic.rs)

## Behavior Contract
- Transcript is append-only. Incoming Gemini Live transcription chunks are appended to `full_transcript`; they are not treated as full replacements.
- Translation never retranslates the whole transcript. It only works on the untranslated tail starting at `last_committed_pos`.
- Translation request dispatch is interval-gated like Windows. The runtime checks for new translation work on a `1500ms` cadence rather than opening a new provider request on every transcript delta.
- If the untranslated tail contains a finished sentence delimiter, only the text through the last delimiter is eligible for commit in that translation pass.
- If no delimiter exists, translation output stays uncommitted until the force-commit path runs.
- Translation output is split into `committed_translation` and `uncommitted_translation`. Display output is `committed + uncommitted`, mirroring the Windows old/new text split.
- Each translation pass sets `last_processed_len` to the current transcript length and clears the previous uncommitted translation before new deltas arrive.
- Normal finished-chunk commit advances `last_committed_pos` by the exact byte count that was translated.
- Force commit appends the current uncommitted translation to committed output, advances `last_committed_pos` to the end of the transcript, and records the `(source, translation)` pair in `translation_history`.
- Target-language changes clear `translation_history` and keep the rest of the state aligned with the Windows runtime.
- Launcher UI is intentionally minimal on Android, but overlay UI is not. Overlay controls, control placement, text animation, waveform behavior, and per-pane layout must follow the Windows realtime overlay contract rather than a mobile-specific redesign.
- Canonical defaults match Windows config:
  - `audio_source=device`
  - `target_language=Vietnamese`
  - `translation_model=cerebras-oss`
  - `transcription_model=gemini`
  - `font_size=16`
- Realtime overlay contract:
  - transcription pane header shows the live waveform canvas, not a fake activity stub
  - translation pane has no title text in the header
  - Google Sans Flex, blur/backdrop treatment, chunked text rendering, and bottom-follow autoscroll match the Windows web overlay
  - `+` and `-` operate independently per pane at runtime
  - the chevron/header collapse affordance overlays content chrome and must not reserve layout width
  - on Android mobile, pane resizing is done with pinch gestures and there is no visible resize button
  - translation pane exposes Read/TTS, translation model, and language controls
  - transcription pane exposes mic/device source and transcription model controls
  - `device` source remains visible even when Android fulfills it through MediaProjection playback capture internally
  - when realtime Read auto-speed is enabled, the visible speed value reflects the current effective playback speed while realtime TTS is actively speaking, not only the saved base slider setting
  - on Android mobile, the transcription and translation panes are separate top-level overlay windows with independent drag and resize behavior
  - Android mobile keeps the portrait default of transcription on top and translation below, but the panes stay detached after launch
  - on Android mobile, header controls must remain horizontally swipeable when they overflow instead of being truncated
  - runtime header actions (`+/-`, mic/device, language, translation model, transcription model, TTS toggle/modal) must preserve the current horizontal header scroll position instead of snapping back to the start
  - runtime header actions must update the already-loaded WebView through discrete JS bridge calls; they must not trigger a full HTML reload just to reflect changed source/model/language/font settings
  - overlay placeholder text, TTS labels, download modal labels, and overlay control tooltips must come from the active mobile UI language bundle rather than fixed English strings
  - when the mobile UI language changes, the overlay chrome must refresh to the new locale on the current session through discrete JS locale updates instead of waiting for a full app restart
  - mobile UI language changes must not force a full pane HTML reload just to swap strings
  - if the translation pane is hidden, mobile must skip opening new translation requests just like Windows skips work when translation visibility is off
- Realtime control contract:
  - translation providers must expose `cerebras-oss`, `google-gemma`, and `google-gtx`
  - `cerebras-oss` resolves to the centralized Cerebras realtime API model `qwen-3-235b-a22b-instruct-2507` on both Windows and Android
  - transcription providers must expose `gemini` and `parakeet`
  - Android may mark Parakeet unavailable, but must not hide it or pretend it is active
  - TTS Read behavior is part of the canonical overlay surface and must not be omitted from the control model
- Android mobile uses a native overlay language picker window instead of relying on the embedded WebView `<select>` popup, because the control must remain usable inside detached overlay windows.

## Failure And Recovery
- Missing BYOK key blocks session start.
- Stop preserves the latest display state in memory until the next session start resets it.
- Overlay lifetime is not tied to a transient provider failure. Recoverable transcription, translation, TTS, or capture failures must not silently close the floating overlay.
- Translation failure must follow the Windows model fallback contract:
  - `cerebras-oss -> google-gtx`
  - `google-gtx -> cerebras-oss`
  - `google-gemma -> random(cerebras-oss, google-gtx)` as Windows currently does
- The realtime provider IDs may stay stable, but their backing API model names must come from centralized model configuration rather than scattered raw strings.
- Translation failure must not silently fall back to whole-transcript translation.
- Timeout gating matches the Windows Gemini path:
  - user silence threshold: `800ms`
  - AI silence threshold: `1000ms`
  - minimum pending source length for force commit: `10` characters

## Fixtures
- Shared fixtures: [parity-fixtures/live-translate/state-machine.json](../../parity-fixtures/live-translate/state-machine.json)
- Shared overlay fixture: [parity-fixtures/live-translate/overlay-bootstrap.json](../../parity-fixtures/live-translate/overlay-bootstrap.json)
- Kotlin parity tests must consume the shared fixture file.
- Rust parity tests must consume the same shared fixture file or validate the same state transitions against `RealtimeState`.

## Deviations
- None for the live-translate state machine.
- Android launcher UI surfaces BYOK entry, a session power button, and a Windows-style global TTS settings modal trigger. The launcher should not replace that modal with a simplified realtime Read settings card.
- The Android launcher `Voice Settings` modal follows the Windows global TTS settings structure:
  - method radio row for Gemini Live, Edge TTS, and Google Translate
  - Gemini section with reading speed, per-language accent conditions, and the Gemini voice grid
  - Google Translate section with the simplified speed-only controls
  - Edge section with pitch, rate, volume, and per-language voice routing entries
- Android target-language choices must use the same ISO-639-1-backed full language list contract as Windows.
- Android launcher does not expose overlay/source/language controls; those live in the overlay itself.
- Android may surface Parakeet as unavailable until a real mobile implementation exists, but it must remain visible and must not fake active transcription.
- Android keeps the Windows glass/tint look and animated blur/mask text appearance. Mobile-specific performance work must preserve the same visible effect rather than swapping in a different animation.
- Android uses a native overlay language picker window for the target-language control while keeping the same ISO-639-1-backed language list and selected-language semantics as Windows.
- Android overlay windows must stay non-focusable so the system IME can still open while the floating panes remain on screen.
- Android routes its own TTS through non-capturable audio attributes and disables playback capture for the app in the manifest, so mobile does not expose the Windows app-selection/per-app-capture UI. This is the approved mobile deviation for avoiding TTS feedback in `device` mode.
- Android realtime Read setting changes must apply to the remaining unread committed text without dropping it. Volume/speed changes must not stay stuck behind already queued stale utterances.
- Android requests transient ducking audio focus while realtime Read is actively speaking so competing app audio is pushed down under TTS. This improves audibility but still depends on the foreground app honoring audio focus.
