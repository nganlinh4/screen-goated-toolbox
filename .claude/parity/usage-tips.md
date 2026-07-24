# Usage Tips Parity

## Canonical Source

- Windows compact entry and localized short labels: [src/gui/settings_ui/footer.rs](../../src/gui/settings_ui/footer.rs)
- Windows full-list popup and bold-marker rendering: [src/gui/app/rendering/footer.rs](../../src/gui/app/rendering/footer.rs)
- Windows localized tip catalog: [src/gui/locale/tips.rs](../../src/gui/locale/tips.rs)
- Windows localized title, label, and hint fields: [src/gui/locale/workspace.rs](../../src/gui/locale/workspace.rs)

## Behavior Contract

- Usage tips have two states: closed and full list open.
- The entry is static. It has no current-tip index, timer, random selection, fade,
  scrolling preview, or animation-driven repaint.
- Windows renders a warning-yellow lightbulb with the short active-locale label:
  `Tips` (`en`), `Mẹo` (`vi`), or `팁` (`ko`).
- Activating the entry opens the complete localized list in catalog order.
- `**bold**` markers are presentation metadata and must render as emphasized text
  without showing the marker characters.
- The English, Vietnamese, and Korean catalogs keep matching count, order, and
  semantic meaning.
- Android content is filtered parity:
  - tips valid on Android keep the Windows meaning or the closest Android wording
  - tips describing a shipped Android equivalent use Android interaction language
  - desktop-only tips with no Android equivalent are omitted

## Deliberate Deviation

- Windows places a compact entry in the footer.
- Android places a static lightbulb entry card in Settings and opens the full list
  in a dialog.
- The Android card may show a localized hint because it has more room than the
  Windows footer. It must not restore an automatically changing tip preview.

## Fixtures

- Shared fixture: [parity-fixtures/mobile-shell/usage-tips.json](../../parity-fixtures/mobile-shell/usage-tips.json)

## Failure And Recovery

- Empty tip lists render an inert entry and never crash either settings surface.
- Unbalanced `**` markers fail catalog validation instead of leaking marker text.
- Closing the full list returns to the same static entry without retaining
  animation or selection state.
