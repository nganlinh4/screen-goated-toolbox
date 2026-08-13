# Usage Tips Parity

## Canonical Source

- Windows compact entry and localized short labels: [src/gui/settings_ui/tips_entry.rs](../../src/gui/settings_ui/tips_entry.rs)
- Windows entry placement: [src/gui/app/rendering/title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Windows categorized modal: [src/gui/settings_ui/tips.rs](../../src/gui/settings_ui/tips.rs)
- Windows localized tip catalog: [src/gui/locale/tips.rs](../../src/gui/locale/tips.rs)
- Windows localized shell fields: [src/gui/locale/workspace.rs](../../src/gui/locale/workspace.rs)
- Android categorized dialog: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/UsageTipsUi.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/UsageTipsUi.kt)

## Editorial Contract

Usage Tips are actionable surprises, not a feature catalog, onboarding checklist,
release notes, or a promotion surface. Include a tip only when it teaches shipped,
stable behavior that is difficult to infer from visible controls:

- a hidden gesture or shortcut
- a prerequisite, constraint, or state invariant
- a recovery or persistence path
- an implicit icon or default meaning
- a useful cross-surface or background interaction

Exclude generic "Open/Use X" copy, visible control inventories, documentation
pointers, transient provider/site claims, and instructions already printed beside
the relevant control. Each tip teaches one behavior, uses current localized UI
labels, and has a stable semantic ID. Re-audit or remove a tip when the product UI
makes its behavior obvious. Every catalog addition or rewrite must be checked
against implementation or test evidence; a plausible description is not enough.

## Category Contract

Both platforms use these stable categories in this order:

1. `capture_shortcuts`
2. `presets_automation`
3. `results_recovery`
4. `models_search`
5. `creative_tools`

Category and tip IDs are semantic metadata, never localized display text. Within
one platform, English, Vietnamese, and Korean keep identical category IDs, tip
IDs, order, and meaning. Android filters out desktop-only behavior and omits any
category left empty by that filtering.

## Behavior Contract

- The entry is static. It has no current-tip index, timer, random selection, fade,
  scrolling preview, or animation-driven repaint.
- Windows renders a warning-yellow lightbulb with the short active-locale label:
  `Tips` (`en`), `Mẹo` (`vi`), or `팁` (`ko`).
- Activating the entry opens the categorized localized catalog.
- Tips are unnumbered because they are independent discoveries, not a sequence.
- `**bold**` markers are presentation metadata and render as emphasized text
  without showing the marker characters.
- Windows remembers the selected category for the app session and falls back to
  the first non-empty category if that selection is unavailable.
- Closing the dialog never changes catalog content or starts automatic rotation.

## Deliberate Presentation Deviations

- Windows places a compact title-bar entry immediately after Settings and uses
  a fixed category rail with one focused reading pane.
- Android places a static lightbulb card in Settings and uses stacked categorized
  sections suited to a narrow screen.
- The Android card may show a localized hint because it has more room than the
  Windows footer entry. It must not restore an automatically changing tip
  preview.

## Fixtures

- Shared fixture: [parity-fixtures/mobile-shell/usage-tips.json](../../parity-fixtures/mobile-shell/usage-tips.json)

## Failure And Recovery

- An empty catalog renders an inert entry and never crashes either settings
  surface.
- Empty categories are omitted.
- Duplicate semantic IDs, locale order drift, and unbalanced `**` markers fail
  catalog validation.
- Closing the full catalog returns to the same static entry.
