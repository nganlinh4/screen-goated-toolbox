# Help Assistant Parity

## Canonical Source
- Windows helper index/runtime logic: [src/gui/settings_ui/help_assistant.rs](../../src/gui/settings_ui/help_assistant.rs)
- Windows launcher placement: [src/gui/app/rendering/title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Windows localized labels and helper copy: [src/gui/locale/en.rs](../../src/gui/locale/en.rs), [src/gui/locale/vi.rs](../../src/gui/locale/vi.rs), [src/gui/locale/ko.rs](../../src/gui/locale/ko.rs)

## Behavior Contract
- The helper assistant fetches and caches the shared `help-index.json` file from GitHub raw:
  - `https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/help-index.json`
- Requests score every index chunk by keyword matches in `path + text`, apply an extra path-match boost, and send the top 20 matching chunks as source context.
- If the question has no searchable terms, requests use the first 20 chunks from the index.
- The model chain is fixed:
  - primary: `gemini-3.1-flash-lite-preview`
  - fallback: `gemma-4-26b-a4b-it`
- Both models use `maxOutputTokens = 4096`, `temperature = 0.7`, and the canonical Gemini thinking config for the selected model.
- Answers must be in the question language, forbid made-up information, forbid “based on the source code” phrasing, require locale-correct UI terms, and return Markdown.
- Windows frames the prompt as the Windows app help assistant. Android frames the prompt as the Android app help assistant and assumes questions are about Android unless the user explicitly mentions Windows.
- Help Assistant requests use long-lived network clients instead of the shorter shared defaults so large help-index fetches and long Gemini generations do not time out prematurely.
- Bucket labels, placeholders, loading messages, and prompt guides are localized in `en`, `vi`, and `ko`.
- Helper failures stay user-visible and recoverable:
  - missing Gemini key returns a visible error answer
  - help-index fetch failure returns a visible error answer
  - Gemini API failure returns a visible error answer

## Deliberate Deviation
- Windows launches the helper from the title bar and uses the text-input overlay for the question.
- Android launches the helper from a dedicated Settings card and uses a native Material 3 dialog/sheet for the question.
- On constrained landscape layouts, Android may tighten the question input layout to preserve space for the ask button.
- In compact landscape, Android may also hide the supporting subtitle and reduce the text-field height to keep the ask button visible without changing the question flow.
- Android still renders the answer in the floating overlay result window, so only the input surface placement differs.

## Fixtures
- Shared fixture: [parity-fixtures/mobile-shell/help-assistant.json](../../parity-fixtures/mobile-shell/help-assistant.json)

## Failure And Recovery
- Android does not introduce a second answer surface for missing overlay permission.
- If the answer overlay cannot be shown, Android should use the same overlay permission/runtime recovery path used by its other overlay features, then retry the helper flow instead of falling back to an in-app answer view.
