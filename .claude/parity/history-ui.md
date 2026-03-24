# History UI Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/app/rendering/title_bar.rs`
  - `src/gui/app/rendering/mod.rs`
  - `src/gui/settings_ui/mod.rs`
- Supporting state/logic:
  - `src/history.rs`
  - `src/gui/app/types.rs`
- UI/output owners:
  - `src/gui/settings_ui/history.rs`
- If Windows uses HTML/CSS/JS/WebView, name that web surface explicitly as canonical:
  - Not applicable. Windows history is a native `egui` surface.

## Behavior Contract
- User-visible flow:
  - History is the 4th main-shell tab.
  - Header contains title, max-items control, search, open-folder, and clear-all actions.
  - Results are newest-first and searchable by result text or timestamp only.
  - Each card shows icon, timestamp, result text, and explicit inline actions.
- State model:
  - `HistoryType = Image | Audio | Text`
  - `HistoryItem = { id, timestamp, itemType, text, mediaPath }`
  - Text items display result text while their backing `.txt` file stores the original input/source text.
  - Max history items default to Windows `50` and clamp to `10..200`.
- Transition rules:
  - Save inserts at the front.
  - Delete removes the card and its backing file.
  - Clear-all removes all cards and backing files.
  - Prune removes oldest items and backing files until the limit is satisfied.
  - Search filters only on `item.text` and `timestamp`.
  - Mobile settings reset restores the History max-items setting to the Windows default `50`.
  - Legacy Android settings snapshots that still carry the old implicit `200` max-items default must normalize back to the Windows default `50` unless the user explicitly changed the limit.
- Output contract:
  - Android uses the same model semantics and ordering as Windows.
  - Android file opens use external intents through `FileProvider`.
  - Android folder opens reuse the existing DocumentsContract-style folder launcher when the history root is externally addressable.

## Failure And Recovery
- Permission/runtime failures:
  - If Android cannot expose the history folder externally, keep the button visible and surface a localized “folder unavailable” toast.
  - If a backing file is missing, opening the item fails gracefully with a localized toast.
- Timeout/retry behavior:
  - Not applicable.
- Stop/reset behavior:
  - Live translate flushes any pending committed segment into history before the session is marked stopped.

## Fixtures
- Shared fixtures:
  - `parity-fixtures/history-ui/state-machine.json`
- Platform-specific tests:
  - Android repository/filter tests
  - Android producer-hook tests for preset and live-save paths

## Deviations
- Android first pass records live-translate history as per-committed text segments because the current Android runtime does not persist reusable WAV blobs like the Windows audio history path.
