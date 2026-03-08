---
name: manage-model-catalog
description: Add, edit, or remove model catalog entries and update capability rules, presets, and hardcoded references that depend on model IDs or API names
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Manage Model Catalog

Safely add, edit, or remove models in screen-goated-toolbox.

## Inputs

Ask the user for any that were not provided:

1. **Operation** — `add`, `edit`, or `remove`.
2. **Scope** — `Text`, `Vision`, `Audio`, or multiple.
3. **Provider** — e.g. `google`, `groq`, `openrouter`, `cerebras`, `ollama`.
4. **Internal ID** — e.g. `text_gemini_flash_lite`.
5. **API model name** (`full_name`) — e.g. `gemini-2.5-flash-lite`.
6. **Labels and quota** — Vietnamese/Korean/English names, quota strings, enabled state, and desired ordering.
7. **Default usage impact** — whether presets, fallbacks, or helper flows should start using the model.

## Steps

1. **Confirm the exact identifiers** before editing. Internal IDs and API model names are both used in this repo, and changing only one often leaves dangling references.

2. **Search the repo first**:
   ```
   rg -n "<internal-id>|<api-model-name>|supports_thinking|model_supports_search_by_name|BlockBuilder::text|get_model_by_id" src .claude -S
   ```
   Also search for nearby sibling models from the same provider/family to copy the correct placement and behavior.

3. **Update the static catalog** in `src/model_config.rs`.
   - Add, edit, or remove the relevant `ModelConfig::new(...)` entry.
   - Keep the correct `ModelType` block (`Vision`, `Text`, `Audio`).
   - Preserve provider grouping and ordering.
   - Treat ordering as behavior, not cosmetics: fallback selection uses list order and often prefers the last matching candidate.

4. **Audit capability rules and provider-specific logic**.
   Check whether the change requires updates in:
   - `src/model_config.rs`
     - `model_is_non_llm()`
     - `model_supports_search_by_name()`
   - `src/api/text/translate.rs`
   - `src/api/text/refine.rs`
   - `src/api/vision.rs`
   - `src/gui/settings_ui/help_assistant.rs`

   Pay special attention to substring checks such as Gemini `supports_thinking` logic. New model families often need explicit handling even when the catalog entry itself is correct.

5. **Audit hardcoded internal ID consumers**.
   Search for the internal ID and update any default or fallback usage that should change:
   - `src/config/preset/defaults/text.rs`
   - `src/config/preset/defaults/image.rs`
   - `src/config/preset/defaults/audio.rs`
   - `src/api/text/refine.rs`
   - Any other `BlockBuilder::text("...")` or `get_model_by_id("...")` call sites found by `rg`

6. **Remove cleanly when deleting a model**.
   If the operation is `remove`, also remove or replace:
   - preset references
   - fallback defaults
   - helper/demo usages
   - capability exceptions tied only to the deleted model

7. **Verify with the repo-approved command for WSL**:
   ```
   ORT_SKIP_DOWNLOAD=1 cargo check --target x86_64-pc-windows-gnu
   ```
   If working on native Windows and the environment supports it, also run:
   ```
   cargo clippy --all-targets
   ```

8. **Report what changed**.
   Summarize:
   - catalog entry changes
   - any capability-rule updates
   - any preset/default updates
   - verification status and any environment-specific limitations

## Key Facts

- The static model catalog lives in `src/model_config.rs`.
- Presets and workflow defaults usually reference the internal model ID, not the API `full_name`.
- Search/grounding support is inferred partly from `full_name` substring checks, so family-level naming changes can alter behavior unexpectedly.
- Gemini request behavior is not fully data-driven; some capabilities are enabled by ad hoc `contains(...)` checks in API call paths.
