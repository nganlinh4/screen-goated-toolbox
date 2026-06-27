# Custom Models Dialog Parity

## Canonical Source
- Windows entrypoints: [src/gui/settings_ui/global/custom_models/mod.rs](../../src/gui/settings_ui/global/custom_models/mod.rs)
- Supporting state/logic: [src/config/types/custom_models.rs](../../src/config/types/custom_models.rs), [src/model_config.rs](../../src/model_config.rs)
- UI/output owners: [src/gui/settings_ui/global/custom_models/openrouter_import.rs](../../src/gui/settings_ui/global/custom_models/openrouter_import.rs)
- Windows does not use HTML/CSS/JS/WebView for this feature.

## Behavior Contract
- User-visible flow: the global settings action opens a model-management dialog with a header, provider-grouped model sections, provider-local add/scan actions, read-only built-in/discovered rows, and editable user rows.
- State model: custom models keep `id`, `provider`, `displayName`, `fullName`, `modelType`, `enabled`, quota text, and optional search support. Built-in and discovered models are visible but locked.
- Transition rules: adding, importing, scanning, editing, enabling, search toggling, and deleting custom models take effect immediately. Adding a provider model creates a unique `custom-<provider>-<slug>` id; imported OpenRouter and scanned Ollama models are skipped when the same provider/full name already exists.
- Output contract: the editable provider order is Gemini, Groq, Cerebras, OpenRouter, Ollama. OpenRouter add/import controls live in the OpenRouter card, and Ollama scan lives in the Ollama card, so the dialog header is reserved for title/description/close only.

## Failure And Recovery
- Permission/runtime failures: none.
- Timeout/retry behavior: OpenRouter and Ollama scan failures stay in the dialog as status text and leave the current model list untouched.
- Stop/reset behavior: closing the dialog does not revert model edits because edits are committed immediately.

## Fixtures
- Shared fixtures: [parity-fixtures/preset-system/custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- Platform-specific tests: Android `CustomModelsParityTest`.

## Deviations
- Android uses the native Compose expressive dialog shell instead of egui, but preserves the Windows provider grouping, locked/user/discovered section contract, and custom-model state fields.
