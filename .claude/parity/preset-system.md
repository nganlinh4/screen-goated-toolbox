# Preset System Parity

## Canonical Source

- Windows editor/catalog: [preset settings](../../src/gui/settings_ui/preset.rs), [preset model](../../src/config/preset/preset.rs)
- Windows execution: [chain](../../src/overlay/process/chain), [text input](../../src/overlay/text_input), [result](../../src/overlay/result), [favorite bubble](../../src/overlay/favorite_bubble)
- Shared model catalog: [model_catalog.json](../../catalog/model_catalog.json)
- Android preset model/runtime: [shared preset](../../mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/preset), [Android preset](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset), [overlay host](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset)

## Data and Editor Contract

- Windows built-ins are canonical seed data. Android persists user overrides by preset ID; restore removes the override.
- Favorite state, ordering, edits, and custom presets are repository-backed, never Compose-only state.
- Android supports preset creation, duplication, deletion, and the current node-graph editor actions. Capability UI must still reflect real runtime support for every block/provider.
- Unknown/corrupt override fields fall back safely to canonical built-ins.
- Hotkeys and controller/master invocation remain Windows-only until Android has a real equivalent.

## Launch and Overlay Contract

- Presets execute from the floating bubble service. Zero favorites shows a localized empty state.
- The panel preserves Windows keep-open, size, multi-column, overlap, animation, drag/reposition, and refresh semantics through a thin Android bridge.
- Input uses the Windows text-input DOM/CSS/message contract, including submit, cancel, history, close, and working microphone input.
- Permission-gated image/audio paths fail before capture, explain the required Android permission, and preserve retry state.
- Image presets support continuous relaunch. Non-image continuous mode remains a documented gap.
- Result windows are session-owned, precreated in loading state, multi-window, and support markdown streaming or raw HTML according to block render mode.
- Reuse Windows markdown fitting/theme/font/table and button-canvas contracts. Preserve text selection, one-finger window drag, two-finger bidirectional content scroll, navigation recovery, and result geometry ownership.
- Edit/refine, undo/redo, share/download, and speaker actions are real Android actions. Do not list implemented actions as placeholders.
- Android still omits the desktop markdown/plain toggle and broom mouse-button variants.

## Provider Contract

- Resolve every internal model ID through generated data from `catalog/model_catalog.json`; call the resolved provider and `full_name`.
- Preserve Windows render-mode, streaming, thinking/search gating, provider-availability, retry, and fallback semantics.
- Provider/auth failures and retryable model failures remain distinct. Retrying an open result updates its loading status.
- Hidden blocks execute without windows; each visible result block owns its own result window.
- Unsupported graph/provider paths return an explicit reason. Never guess from ID prefixes.

## Fixtures

- [audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)
- [catalog-overrides.json](../../parity-fixtures/preset-system/catalog-overrides.json)
- [custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- [gemini-live-socket-protocol.json](../../parity-fixtures/preset-system/gemini-live-socket-protocol.json)
- [node-graph-editor.json](../../parity-fixtures/preset-system/node-graph-editor.json)
- [result-overlay.json](../../parity-fixtures/preset-system/result-overlay.json)
- [retry-runtime.json](../../parity-fixtures/preset-system/retry-runtime.json)
- [text-input-overlay.json](../../parity-fixtures/preset-system/text-input-overlay.json)
- [text-provider-routing.json](../../parity-fixtures/preset-system/text-provider-routing.json)

## Known Contract Debt

- `retry-runtime.json` and the catalog retry chain still name a retired Flash-Lite preview model.
- `result-overlay.json` still marks implemented result actions unsupported.
- `text-input-overlay.json` still labels microphone input deferred.
- One `catalog-overrides.json` case name says HTML is a placeholder although its expected result is supported.

Treat these as fixture/source synchronization work, not as permission to restore old behavior.
