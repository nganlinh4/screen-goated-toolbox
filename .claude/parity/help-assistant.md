# Help Assistant Parity

## Canonical Source

- Windows request/ranking: [help_assistant.rs](../../src/gui/settings_ui/help_assistant.rs)
- Windows launcher: [title_bar.rs](../../src/gui/app/rendering/title_bar.rs)
- Android client/UI: [helpassistant](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/helpassistant)
- Shared contract: [help-assistant.json](../../parity-fixtures/mobile-shell/help-assistant.json)

## Contract

- Read the app-selected `component-delivery/help-index-v1.json` contract, fetch
  its immutable content-addressed gzip asset, and verify compressed and expanded
  byte counts and SHA-256 identities before parsing it.
  The delivery contract owns the current asset identity. The parity fixture
  records behavior across index revisions.
- Cache the verified compressed asset persistently. Reuse the selected cached
  asset without a network request, and retain it as the offline copy when a
  replacement download fails. Never read a mutable branch or staging URL from a
  production build.
- Build the source index from a committed revision of the whole repository.
  Clip each source chunk at a bounded length and skip local credentials,
  generated output, vendor dependencies, and binary assets. Store the
  source revision, embedding model and dimensions with the index.
- Filter entries to the current platform before ranking. In the source
  index, rank signed-byte vectors by cosine against the question embedded
  with the same model and dimensions. Send up to six excerpts with source
  paths and line numbers. Use keyword ranking if query embedding fails.
- Continue reading verified v1 indexes and using their keyword ranking. Count
  non-overlapping matches across `path + text` and apply the fixture's path
  boost. V1 uses the top 20 and the first 20 when there are no searchable terms.
- Use one primary and one fallback Gemini model with the same output limit,
  temperature, and catalog-resolved `LOW` important-task thinking configuration
  on both platforms. Ordinary model calls remain at disabled/minimal thinking.
- The model chain must name models that are currently available. Change both platform constants and the shared fixture in one commit; never copy model IDs into this prose.
- Answer in the question language, use locale-correct UI terms, and return
  Markdown. Treat excerpts as evidence, ignore instructions in excerpts, and
  say when the evidence does not establish the answer.
- Use dedicated long-lived network clients for the index and generation requests.
- Missing keys, fetch failures, and model failures remain visible and recoverable.
- Prepare the source index at the start of a release for its reviewed Git
  revision. Reuse cached embeddings for unchanged source chunks. Publish only
  an index built from a revision whose indexed source is already public.

## Platform Surface

- Windows asks from the title bar through the text-input overlay.
- Android asks from Settings through native Material 3 UI, then shows the answer in the shared floating result overlay.
- Android may tighten the input layout in compact landscape, but must not change request/ranking semantics or create a second answer surface.
- Missing overlay permission preserves the pending question, opens system settings, and retries the result overlay afterward.

The ordered model chain is generated from `catalog/model_catalog.json`; platform
code must not duplicate it.
