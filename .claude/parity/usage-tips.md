# Usage Tips Parity

## Canonical Source
- Windows tip rotation and fade timing: [src/gui/app/logic.rs](../../src/gui/app/logic.rs)
- Windows footer preview and full-list popup: [src/gui/app/rendering/footer.rs](../../src/gui/app/rendering/footer.rs)
- Windows footer click behavior and bold-marker rendering: [src/gui/settings_ui/footer.rs](../../src/gui/settings_ui/footer.rs)
- Windows localized tip strings: [src/gui/locale/en.rs](../../src/gui/locale/en.rs), [src/gui/locale/vi.rs](../../src/gui/locale/vi.rs), [src/gui/locale/ko.rs](../../src/gui/locale/ko.rs)

## Behavior Contract
- Android keeps the Windows usage-tips state model:
  - preview shows one tip at a time
  - first preview fades in from alpha `0.0` to `1.0`
  - visible hold duration is `2.0 + tip.len() * 0.06` seconds
  - fade duration is `0.5` seconds
  - next tip selection is randomized and must not repeat the current tip when more than one tip exists
- Android keeps the Windows `**bold**` marker contract for both the preview text and the full tip list.
- Android localizes tips through the active mobile UI locale bundle for `en`, `vi`, and `ko`.
- Android content is filtered parity:
  - tips valid on Android keep the Windows wording or the closest Android wording
  - tips that describe a shipped Android equivalent may be rewritten to match Android interaction language
  - desktop-only tips with no Android equivalent are omitted

## Deliberate Deviation
- Windows renders the preview in the footer and opens the full list from footer text.
- Android renders the preview in a dedicated Settings card and opens the full list from that card.
- This is a placement-only deviation; timing, bold rendering, localization, random rotation, and full-list access remain Windows-canonical.

## Fixtures
- Shared fixture: [parity-fixtures/mobile-shell/usage-tips.json](../../parity-fixtures/mobile-shell/usage-tips.json)

## Failure And Recovery
- Empty tip lists must render an inert card state and never crash the Settings screen.
- A single-item tip list must never try to rotate to a different index.
