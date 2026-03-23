# Help Assistant Parity

## Canonical Source
- Windows helper bucket/runtime logic: [src/gui/settings_ui/help_assistant.rs](../../src/gui/settings_ui/help_assistant.rs)
- Windows launcher placement: [src/gui/app/rendering/title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Windows localized labels and helper copy: [src/gui/locale/en.rs](../../src/gui/locale/en.rs), [src/gui/locale/vi.rs](../../src/gui/locale/vi.rs), [src/gui/locale/ko.rs](../../src/gui/locale/ko.rs)

## Behavior Contract
- The helper assistant uses three fixed context buckets in this order:
  - `screen-record`
  - `android`
  - `rest`
- Each bucket fetches a prebuilt repomix XML from GitHub raw:
  - `repomix-screen-recorder.xml`
  - `repomix-android.xml`
  - `repomix-rest.xml`
- Both Windows and Android use the same Gemini request contract:
  - model endpoint family: `gemini-3-flash-preview:generateContent`
  - same system prompt intent: answer in the question language, concise, no made-up information, no “based on the source code” phrasing, correct locale UI terms, Markdown output
- Bucket labels, placeholders, loading messages, and prompt guides are localized in `en`, `vi`, and `ko`.
- Helper failures stay user-visible and recoverable:
  - missing Gemini key returns a visible error answer
  - GitHub raw fetch failure returns a visible error answer
  - Gemini API failure returns a visible error answer

## Deliberate Deviation
- Windows launches the helper from the title bar, uses the preset wheel for bucket choice, and text-input overlay for the question.
- Android launches the helper from a dedicated Settings card and uses a native Material 3 dialog/sheet for bucket choice and question entry.
- Android still renders the answer in the floating overlay result window, so only the input surface placement differs.

## Fixtures
- Shared fixture: [parity-fixtures/mobile-shell/help-assistant.json](../../parity-fixtures/mobile-shell/help-assistant.json)

## Failure And Recovery
- Android does not introduce a second answer surface for missing overlay permission.
- If the answer overlay cannot be shown, Android should use the same overlay permission/runtime recovery path used by its other overlay features, then retry the helper flow instead of falling back to an in-app answer view.
