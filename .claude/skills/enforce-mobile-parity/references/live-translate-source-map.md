# Live Translate Source Map

Use these files first when mobile live translate must match Windows:

- [src/overlay/realtime_html.rs](../../../src/overlay/realtime_html.rs)
- [src/api/realtime_audio/state.rs](../../../src/api/realtime_audio/state.rs)
- [src/api/realtime_audio/transcription.rs](../../../src/api/realtime_audio/transcription.rs)
- [src/api/realtime_audio/translation.rs](../../../src/api/realtime_audio/translation.rs)
- [src/api/realtime_audio/websocket.rs](../../../src/api/realtime_audio/websocket.rs)
- [src/overlay/realtime_webview/state.rs](../../../src/overlay/realtime_webview/state.rs)
- [src/overlay/html_components/js_logic.rs](../../../src/overlay/html_components/js_logic.rs)

Key invariants:

- Transcript appends; it is not replaced.
- Translation runs on the untranslated tail, not the whole transcript.
- Translation has committed and uncommitted phases.
- Commit pointers advance by the exact translated source length.
- Force commit is silence-driven and records translation history.
