# Windows shortcut bindings

`Hotkey.code` and `Hotkey.modifiers` are the binding identity. Use
`Hotkey::display_name()` for a configured shortcut and `names::key_name()` for
an individual Windows virtual key. Never derive a label from an egui debug
name, a DOM physical code, or a saved `name` string.

The JSON contract remains `{code, modifiers, name}`. Deserialization accepts
older documents, ignores their display name, and preserves code/modifiers.
Serialization computes the current label, so mini-app chips, exports and saved
configuration use the same formatter as native settings and conflict messages.

## Naming and capture

- Modifier order is Ctrl, Alt, Shift, Win, separated by ` + `. Registration
  flags such as `MOD_NOREPEAT` do not appear in the label.
- Letters and main-row digits use their logical Windows VK names. Numpad
  digits/operators are identified explicitly; F1–F24, navigation, editing,
  browser, media, mouse buttons and standard IME keys have shared labels.
- OEM punctuation is resolved through Windows using the active input layout.
  Dead-key accents are displayed without changing the keyboard's composition
  state. Unmapped keys retain an unambiguous `VK 0xNN` label.
- The focused native settings window captures Windows key messages before
  egui merges numpad/main-row identities. Plain Esc cancels capture; modified
  Esc remains a binding. Shortcut dispatch is suspended while capturing and
  restored when capture ends or the settings window loses focus.
- WebView capture uses `web_binding::from_web_event`. Physical DOM positions
  are translated through the input layout, named keys use Windows VK values,
  and malformed/unsupported codes are rejected instead of becoming a different
  key. Numpad navigation follows the actual Num Lock interpretation.

The binding model supports one VK plus the four modifier flags. It cannot
distinguish main Enter from numpad Enter, left from right modifier requirements,
or represent multi-step chords. Naming a combination does not guarantee that
Windows will register it: reserved and already-registered combinations can fail.

## Consumers audited

| Surface | Label owner |
| --- | --- |
| Preset, screen translation, live translation and Computer Control settings | `Hotkey::display_name()` |
| Conflict messages and registration diagnostics | `Hotkey::display_name()` |
| Recording, text input, selection and continuous-mode hints | Canonical labels; `names::with_escape()` for exit hints |
| Screen Recorder and Translation Gummy shortcut chips | Serialized `Hotkey` values |
| Recorder keystrokes | Shared Rust key/combo formatter, captured input layout, complete `RawInputEvent.label` |

The recorder worker imports the same Rust modules through path references.
New keystroke labels flow unchanged into the shared preview/export events.
Older input recordings without `label` retain their existing frontend reader;
already-authored keystroke labels in saved projects are not rewritten.

Fixed, non-configurable editor accelerators remain action-specific UI copy.
Computer-control key injection is an execution contract, not a display-name
source. Android does not register these Windows global shortcuts; the exported
JSON shape remains compatible.

## Verification

From the repository root:

```powershell
cargo test --bin screen-goated-toolbox hotkey
node scripts/test-recording-overlay.cjs
cargo check --manifest-path native/recorder_worker/Cargo.toml --locked
```

The browser check uses Playwright Chromium and the repository font to
check paused waveform stability and adaptive hint condensation in both themes.
Set `SGT_RECORDING_UI_EVIDENCE_DIR` to an external directory to save renders.
Run the development checkpoint in `../../docs/DEVELOPMENT.md` and the recorder
checks in `../../screen-record/README.md` after cross-subsystem changes. Recorder
byte changes follow `../../docs/COMPONENT_DELIVERY.md` before host release.
