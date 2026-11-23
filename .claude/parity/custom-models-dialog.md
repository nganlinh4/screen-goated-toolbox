# Custom Models Dialog Parity

## Canonical Source
- Windows entrypoints: [src/gui/settings_ui/global/custom_models/mod.rs](../../src/gui/settings_ui/global/custom_models/mod.rs)
- Supporting state/logic: [src/config/types/custom_models.rs](../../src/config/types/custom_models.rs), [src/model_config.rs](../../src/model_config.rs)
- UI/output owners: [src/gui/settings_ui/global/custom_models/openrouter_import.rs](../../src/gui/settings_ui/global/custom_models/openrouter_import.rs)
- Windows does not use HTML/CSS/JS/WebView for this feature.

## Behavior Contract
- User-visible flow: the global settings action opens a model-management dialog with a header, provider-grouped model sections, provider-local add/scan actions, read-only built-in/discovered rows, and editable user rows.
- State model: custom models keep `id`, `provider`, `displayName`, `fullName`, `modelType`, `enabled`, quota text, and optional search support. Built-in and discovered models are visible but locked.
- Transition rules: adding, importing, scanning, editing, enabling, search toggling, and deleting custom models update the editor immediately. Android publishes a model catalog only after its background settings write is acknowledged. Adding a provider model creates a unique `custom-<provider>-<slug>` id; imported OpenRouter and scanned Ollama models are skipped when the same provider/full name already exists.
- Published model collections are owned snapshots. Mutating a caller's input or
  a previously returned collection cannot silently change the active catalog.
- Editor synchronization must preserve accepted edits when observations arrive
  late. A newer accepted snapshot takes precedence over older observations;
  later external edits and save outcomes must still become visible.
- An edit based on an older model list is rejected if the current list changed;
  the editor shows the current list and asks the user to review and try again.
  A save-state change alone does not make an otherwise current edit stale.
- Row actions target the model that was displayed, even if another row was
  removed. A delayed action must not change a different or replacement row;
  an edit to one field must preserve newer values in the other fields.
- Android scan imports deduplicate against both the existing draft and the
  current scan, and allocate every added ID against both sets.
- OpenRouter import assigns Vision only when `architecture.input_modalities` contains the exact `image` modality; otherwise it assigns Text. Pricing, output modalities, descriptions, and model names do not establish image-input support. Existing custom models retain their saved type.
- OpenRouter import search filters the whole scanned list by a case-insensitive substring of the model name or id. Android renders at most 30 matches at a time, so any scanned model can be reached by searching.
- Output contract: the editable provider order is Gemini, Groq, OpenRouter, Ollama. OpenRouter add/import controls live in the OpenRouter card, and Ollama scan lives in the Ollama card, so the dialog header is reserved for title/description/close only.

## Failure And Recovery
- If saved custom-model data cannot be decoded, Android preserves it, marks
  custom models unavailable, disables their edits, and keeps unrelated features
  usable. A secure-store access failure still blocks secure bootstrap.
- Android serializes settings writes and keeps at most the latest queued edit.
  If a write fails or throws, it retains the last acknowledged catalog and the
  latest pending editor snapshot, reports an uncertain outcome, and blocks new
  edits. The user can retry that pending snapshot. Pending edits are process-local.
  A cancelled write with a still-active writer is also uncertain; cancellation
  of the writer itself continues to propagate.
- Timeout/retry behavior: OpenRouter and Ollama scan failures stay in the dialog as status text and leave the current model list untouched.
- Android scan cancellation cancels the owning HTTP call; a late response is
  closed and cannot update a retired editor. Cancellation remains cancellation.
- Scan JSON bodies are limited to 10 MiB, matching the Windows HTTP client's
  default JSON read limit. An oversized or incomplete response is not imported.
- Stop/reset behavior: closing the dialog does not revert model edits because edits are committed immediately.
  A callback from a closed dialog or a retired editor cannot submit another
  edit or retry after a new editor is shown.

## Fixtures
- Shared fixtures: [parity-fixtures/preset-system/custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- Snapshot ownership: [custom-model-ownership.json](../../parity-fixtures/preset-system/custom-model-ownership.json)
- Persistence loading: [custom-model-persistence.json](../../parity-fixtures/preset-system/custom-model-persistence.json)
- Save failures: [custom-model-write-failures.json](../../parity-fixtures/preset-system/custom-model-write-failures.json)
- Ordered saves: [custom-model-write-order.json](../../parity-fixtures/preset-system/custom-model-write-order.json)
- Editor observations: [custom-model-editor-observations.json](../../parity-fixtures/preset-system/custom-model-editor-observations.json)
- Row actions: [custom-model-row-actions.json](../../parity-fixtures/preset-system/custom-model-row-actions.json)
- Scan imports: [custom-model-scan-imports.json](../../parity-fixtures/preset-system/custom-model-scan-imports.json)
- Scan cancellation: [custom-model-scan-cancellation.json](../../parity-fixtures/preset-system/custom-model-scan-cancellation.json)
- Scan response limits: [custom-model-scan-response-limit.json](../../parity-fixtures/preset-system/custom-model-scan-response-limit.json)
- Platform-specific tests: Android `CustomModelsParityTest`.

## Deviations
- Android uses the native Compose expressive dialog shell instead of egui, but preserves the Windows provider grouping, locked/user/discovered section contract, and custom-model state fields.
