# Mobile Shell Parity

## Canonical Source
- Windows title-bar theme cycle + language switcher: [src/gui/app/rendering/title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Windows config defaults for theme and UI language: [src/config/config.rs](../../src/config/config.rs)
- Windows theme and system-language enums/helpers: [src/config/types/enums.rs](../../src/config/types/enums.rs)
- Windows locale string bundles: [src/gui/locale/mod.rs](../../src/gui/locale/mod.rs), [src/gui/locale/en.rs](../../src/gui/locale/en.rs), [src/gui/locale/vi.rs](../../src/gui/locale/vi.rs), [src/gui/locale/ko.rs](../../src/gui/locale/ko.rs)
- Windows TTS settings preview behavior: [src/gui/settings_ui/global/tts_settings.rs](../../src/gui/settings_ui/global/tts_settings.rs)
- Windows app/tray icon assets: [assets/app-icon-small.png](../../assets/app-icon-small.png), [assets/app-icon-small-light.png](../../assets/app-icon-small-light.png), [assets/tray_icon.png](../../assets/tray_icon.png), [assets/tray_icon-light.png](../../assets/tray_icon-light.png)

## Behavior Contract
- Mobile keeps the same top-level UI language choices as Windows:
  - `en`
  - `vi`
  - `ko`
- Mobile keeps the same top-level theme choices as Windows:
  - `System`
  - `Dark`
  - `Light`
- Theme-cycle behavior matches Windows exactly:
  - `System -> Dark -> Light -> System`
- The launcher and main TTS settings UI must read visible labels from the active UI language bundle rather than from hard-coded English strings.
- TTS preview behavior matches the Windows contract:
  - preview text comes from the active locale bundle's `tts_preview_texts`
  - `{}`
  placeholders are replaced with the selected voice name
  - preview selection should avoid repeating the immediately previous preview entry when multiple entries exist
- Mobile in-app branding uses the Windows light/dark app-icon pair:
  - dark surfaces use the dark Windows icon asset
  - light surfaces use the light Windows icon asset
  - launcher resources may stay Android-stable, but in-app shell, dialogs, and notification branding must come from the Windows asset pair
- Mobile shell layout stays Windows-structured but uses Android-native expressive composition:
  - top command bar for language, theme, and branding
  - global settings surface for credentials, live control, and TTS settings
  - honest placeholders for future Windows sections instead of fake-working mobile-only screens
- Mobile may keep Android-native rendering, but the language/theme state model and localized preview text source must match Windows.

## Failure And Recovery
- Unsupported or unknown UI language codes fall back to `en`.
- Theme `System` resolves from the current Android system dark-mode state.
- Existing persisted settings from older mobile builds must still load with safe defaults.

## Fixtures
- Shared fixtures: [parity-fixtures/mobile-shell/ui-language-theme.json](../../parity-fixtures/mobile-shell/ui-language-theme.json)
- Android unit tests must at minimum cover theme-cycle order and locale-preview lookup.

## Deviations
- Mobile uses Android-native Compose controls instead of egui widgets.
- Windows currently maps some non-`en/vi/ko` system locales such as `ja` and `zh` internally; mobile launcher selection remains the same three-option UI requested for parity with the visible Windows title bar.
