# Help Assistant Parity

## Canonical Source

- Windows request/ranking: [help_assistant.rs](../../src/gui/settings_ui/help_assistant.rs)
- Windows launcher: [title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Android client/UI: [helpassistant](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/helpassistant)
- Shared contract: [help-assistant.json](../../parity-fixtures/mobile-shell/help-assistant.json)

## Contract

- Fetch and cache the tracked `help-index.json` from the repository raw URL.
- Rank every chunk by non-overlapping question-term matches across `path + text`, apply the fixture's path boost, and send the top 20. With no searchable terms, use the first 20 chunks.
- Use one primary and one fallback Gemini model with the same output limit, temperature, and model-appropriate thinking configuration on both platforms.
- The model chain must name models that are currently available. Change both platform constants and the shared fixture in one commit; never copy model IDs into this prose.
- Answer in the question language, use locale-correct UI terms, return Markdown, and prohibit invented facts or source-code framing.
- Use dedicated long-lived network clients for the index and generation requests.
- Missing keys, fetch failures, and model failures remain visible and recoverable.

## Platform Surface

- Windows asks from the title bar through the text-input overlay.
- Android asks from Settings through native Material 3 UI, then shows the answer in the shared floating result overlay.
- Android may tighten the input layout in compact landscape, but must not change request/ranking semantics or create a second answer surface.
- Missing overlay permission preserves the pending question, opens system settings, and retries the result overlay afterward.

The ordered model chain is generated from `catalog/model_catalog.json`; platform
code must not duplicate it.
