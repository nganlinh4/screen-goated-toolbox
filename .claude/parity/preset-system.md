# Preset System Parity

## Canonical Source
- Windows preset editor and field visibility logic: [src/gui/settings_ui/preset.rs](../../src/gui/settings_ui/preset.rs)
- Windows preset collection management: [src/gui/settings_ui/sidebar.rs](../../src/gui/settings_ui/sidebar.rs)
- Windows preset data model: [src/config/preset/preset.rs](../../src/config/preset/preset.rs)
- Windows chain execution entrypoint: [src/overlay/process/chain/mod.rs](../../src/overlay/process/chain/mod.rs)
- Windows text-input overlay runtime: [src/overlay/text_input/mod.rs](../../src/overlay/text_input/mod.rs)
- Windows favorite bubble launcher: [src/overlay/favorite_bubble/mod.rs](../../src/overlay/favorite_bubble/mod.rs)
- Windows markdown result runtime: [src/overlay/result/markdown_view/mod.rs](../../src/overlay/result/markdown_view/mod.rs)
- Windows result button canvas: [src/overlay/result/button_canvas/mod.rs](../../src/overlay/result/button_canvas/mod.rs)

## Behavior Contract
- Android treats the Windows built-in preset catalog as canonical seed data.
- Built-in presets are immutable defaults; Android user changes are stored as overrides keyed by preset ID.
- Restore default removes the Android override and returns the resolved preset to the Windows default value.
- Android must not expose editable controls for preset behaviors that do not have real Android runtime support yet.
- Unsupported Windows-only behavior must render as read-only placeholders with an explicit reason.
- Favorite state is repository-backed and persisted; it must not live only in Compose memory.
- Android may keep a preset details/inspector screen before a full editor, but it must not pretend node-graph editing, hotkeys, controller mode, audio capture, or auto-paste work if they do not.
- Android wave 1 execution is limited to text-input presets whose graphs are text-only. Text-selection capture and overlay-style input remain placeholders until Android has a real runtime for them.
- Android preset execution must run from the floating bubble service, not from the main app inspector UI.
- Android bubble runtime is `favorites only` in wave 1.
- The bubble panel honors the Windows keep-open toggle:
  - default launch path closes the panel when a preset is launched
  - keep-open launches supported presets without dismissing the panel
- The bubble size controls must persist and resize the actual floating bubble using the Windows min/max/step semantics.
- Supported Android bubble launches open a floating text-input overlay first, then stream into a floating markdown result overlay with a separate floating button canvas.
- The floating text-input overlay is a Windows-canonical web surface:
  - Windows source of truth: [src/overlay/text_input/styles.rs](../../src/overlay/text_input/styles.rs), [src/overlay/text_input/messages.rs](../../src/overlay/text_input/messages.rs), and [src/overlay/text_input/window.rs](../../src/overlay/text_input/window.rs)
  - Android must use the same DOM/CSS/JS contract through a builder/shim layer instead of a custom `input.html/css/js` redesign
  - Android-specific changes are limited to bridge transport, touch drag shims, and explicitly documented unsupported controls
  - Title, footer, placeholder, entry/exit animation, theme variables, and button layout must stay aligned with the Windows text-input surface
  - The Android input overlay should accept the Windows message contract for `submit:*`, `cancel`, `close_window`, `history_up:*`, `history_down:*`, and `mic`
- If the bubble is opened with zero favorite presets, Android must surface a localized empty-favorites message instead of crashing.
- The favorite bubble panel is a Windows-canonical web surface:
  - Windows source of truth: [src/overlay/favorite_bubble/html.rs](../../src/overlay/favorite_bubble/html.rs)
  - Android must follow the Live Translate builder/shim pattern for this panel instead of hand-designing a separate mobile variant
  - keep-open row, size controls, DOM structure, pill spacing, icon treatment, text-fit behavior, and panel motion/animations should come from the Windows web contract unless this spec explicitly documents a deviation
  - Android-specific changes are limited to bridge transport, touch/mobile interaction shims, and explicitly unsupported controls
  - for larger favorite counts, the panel should follow the Windows multi-column rule instead of degenerating into a single very tall column; on mobile, column count may be capped by available screen width
  - The floating bubble itself must remain tappable and draggable while the panel is open; panel z-order or hit handling must not block bubble interaction
  - Closing the panel from the bubble should use the Windows-style web close animation and only destroy the Android overlay window after the web surface emits its close completion signal
  - If favorites change while the panel is open, Android must rebuild the panel contents and geometry so the visible pills stay in sync instead of disappearing or clipping
  - Dragging the bubble must keep the panel open and reposition it with the bubble, matching the Windows expanded-panel behavior
  - The Android bubble may expose a live-translate-style drag-to-dismiss target, but dropping onto it must be equivalent to turning the Quick Settings bubble service off
- Android preset overlays use markdown view only in wave 1. HTML-output presets remain placeholders until Android has the Windows-style raw HTML result runtime.
- Android result overlays should reuse the Windows markdown CSS, fit script, and button-canvas web contract from the shared HTML/WebView layer instead of re-implementing the layout in Compose.
- Android result overlays are multi-window:
  - each visible text block with `showOverlay = true` and `renderMode = markdown|markdown_stream` spawns its own result overlay
  - hidden text blocks still execute but do not create result windows
  - the first visible result window uses the preset saved geometry; subsequent visible windows snake away from the previous visible result window
- Android result windows must not use a fake title/status shell. The result window should be a Windows-derived markdown surface with touch shims only.
- Android button canvas uses the shared Windows button-canvas web contract with an Android touch reveal model:
  - tapping a result window reveals that window's controls
  - controls linger for roughly 2 seconds after the last interaction, then fade
  - Android may use a bounded active-window canvas instead of a fullscreen canvas if that is required to preserve reliable touch/click-through behavior on phone overlays
  - any Android-bounded canvas must stay visually identical to the Windows button canvas and remain anchored to the currently active result window
- On Android/touch, the markdown-mode button is omitted because mobile result overlays are markdown-only in this wave.
- On Android/touch, the broom button is omitted and result-window dismissal uses the shared drag-to-dismiss `X` target instead.
- Android keeps the Windows button-canvas chrome visible, but unsupported actions must be explicit placeholders rather than fake implementations.

## Failure And Recovery
- Corrupt preset override storage falls back to canonical built-ins with no overrides applied.
- Unknown override fields are ignored on load.
- Restore default must be safe for any built-in preset, even if no override exists.

## Fixtures
- Shared fixtures: [parity-fixtures/preset-system/catalog-overrides.json](../../parity-fixtures/preset-system/catalog-overrides.json)
- Android unit tests should cover override merge, restore default, and placeholder capability resolution.

## Deviations
- Android wave 1 keeps custom preset create/clone/delete/reorder as placeholders.
- Android wave 1 keeps hotkeys, controller/master invocation, image capture, selected-text capture, mic/device capture, realtime audio, raw HTML result rendering, and auto-paste as placeholders until real runtime exists.
- Android wave 1 does not expose the Windows markdown/plain-text result toggle in the floating button canvas; the mobile overlay stays markdown-only until a real alternate render mode exists.
- Android wave 1 keeps these result/button-canvas actions as visible placeholders:
  - edit/refine
  - undo/redo
  - download
  - speaker
  - broom group/all mouse-button variants
- Android favorite bubble still has known parity gaps versus Windows:
  - panel `trigger_continuous` does not yet enter the Windows continuous-mode runtime; Android still routes that path through the normal preset launch flow
- Android text-input overlay still has known parity gaps versus Windows:
  - `mic` remains a placeholder until Android has the preset-linked recording/runtime path behind that control
- On Android/touch, the text-input footer row may be omitted and the action buttons may be compacted inward so the overlay remains truthful and usable within the smaller mobile window. This is an accepted mobile interaction adaptation, not a parity bug.
- On Android/touch, the keep-open row may remain visible instead of hover-revealed; this is an accepted mobile interaction adaptation, not a parity bug.
- Android bubble opacity should stay fully active while the panel is expanded or within roughly one second of the last bubble/panel interaction, then return to the Windows inactive-opacity baseline.
- On Android/touch, full result-window dragging is implemented as long-press then drag anywhere on the result surface so normal taps and links remain usable.
