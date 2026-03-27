# Help Assistant Parity

## Canonical Source
- Windows helper bucket/runtime logic: [src/gui/settings_ui/help_assistant.rs](../../src/gui/settings_ui/help_assistant.rs)
- Windows launcher placement: [src/gui/app/rendering/title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Windows localized labels and helper copy: [src/gui/locale/en.rs](../../src/gui/locale/en.rs), [src/gui/locale/vi.rs](../../src/gui/locale/vi.rs), [src/gui/locale/ko.rs](../../src/gui/locale/ko.rs)

## Behavior Contract
- The helper assistant has two answer modes shared across platforms:
  - `quick`
  - `detailed`
- The answer mode contract is:
  - `quick` uses model endpoint `gemini-3.1-flash-lite-preview:generateContent`
  - `detailed` uses model endpoint `gemini-3-flash-preview:generateContent`
  - both modes answer in the question language, forbid made-up information, forbid “based on the source code” phrasing, require locale-correct UI terms, and return Markdown
  - `quick` prefers shorter, direct answers
  - `detailed` prefers fuller answers with clearer steps and context
- Windows uses three fixed context buckets in this order:
  - `screen-record`
  - `android`
  - `rest`
- Windows bucket choice fetches a prebuilt repomix XML from GitHub raw:
  - `repomix-screen-recorder.xml`
  - `repomix-android.xml`
  - `repomix-rest.xml`
- Android currently launches this helper as an Android-scoped entry and uses the `android` bucket for its request context.
- Help Assistant requests use long-lived network clients instead of the shorter shared defaults so large repomix fetches and long Gemini generations do not time out prematurely.
- Bucket labels, placeholders, loading messages, and prompt guides are localized in `en`, `vi`, and `ko`.
- Helper failures stay user-visible and recoverable:
  - missing Gemini key returns a visible error answer
  - GitHub raw fetch failure returns a visible error answer
  - Gemini API failure returns a visible error answer

## Deliberate Deviation
- Windows launches the helper from the title bar, uses the preset wheel for bucket choice, then a second preset wheel for answer mode, and finally the text-input overlay for the question.
- Android launches the helper from a dedicated Settings card and uses a native Material 3 dialog/sheet with the answer-mode chooser placed above the text field.
- Android still renders the answer in the floating overlay result window, so only the input surface placement differs.

## Fixtures
- Shared fixture: [parity-fixtures/mobile-shell/help-assistant.json](../../parity-fixtures/mobile-shell/help-assistant.json)

## Failure And Recovery
- Android does not introduce a second answer surface for missing overlay permission.
- If the answer overlay cannot be shown, Android should use the same overlay permission/runtime recovery path used by its other overlay features, then retry the helper flow instead of falling back to an in-app answer view.
