---
name: manage-model-catalog
description: Add, edit, rename, reorder, or remove models through the canonical catalog and every generated/runtime consumer.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Manage Model Catalog

## Source of Truth

`catalog/model_catalog.json` owns model IDs, API names, defaults, aliases, order, and shared provider metadata. Never start by hardcoding a model in a UI or platform runtime.

`catalog/README.md` owns the durable internal-ID namespace, localized-name
prefixes, performance metadata, migration policy, and priority policy. Apply
those rules to every catalog change; do not invent a feature-local convention.

## Workflow

1. Confirm operation, model type, provider, internal ID, API `full_name`, labels, enabled state, order, and default/fallback impact.
2. Check the provider's current official model/deprecation documentation.
3. Search both identifiers and sibling-family capability logic:

```powershell
rg -n '<internal-id>|<api-model>|supports_thinking|supports_search|model_is_non_llm|get_model_by_id' catalog src mobile scripts parity-fixtures .claude
```

4. Edit the manifest and every relevant manifest section: constants, defaults, provider defaults, priority chains, aliases, TTS/Live lists, and non-LLM/search capability sets.
5. Audit feature-specific request logic in `src/api/`, `src/overlay/`, and Android clients. A catalog entry does not automatically make a wire protocol compatible.
6. Update presets, parity fixtures, and tests that use the internal ID. The
   catalog intentionally has no permanent model-ID migration table; unknown
   saved IDs fall back by modality.
7. Regenerate Android outputs. Gradle does this during normal builds; for direct inspection:

```powershell
py -3 scripts\generate_android_preset_model_catalog.py `
  --manifest-source catalog\model_catalog.json `
  --preset-output $env:TEMP\GeneratedPresetModelCatalogData.kt `
  --preset-defaults-output $env:TEMP\GeneratedPresetDefaultModels.kt `
  --live-output $env:TEMP\GeneratedLiveModelCatalog.kt
```

Validate without generating files:

```powershell
py -3 scripts\generate_android_preset_model_catalog.py --manifest-source catalog\model_catalog.json --validate-only
```

The validator and Cargo build reject duplicate IDs, permanent migration tables,
incomplete lifecycle metadata, and deprecated/retired runtime defaults.

8. For removal or an intentional namespace rewrite, prove no dangling active
   references remain with `rg`.
9. Run focused tests, then repository validation from `AGENTS.md`. For Android catalog changes, run the relevant Gradle compile/unit tests from `mobile/README.md`.

## High-Risk Owners

- Windows catalog wrapper: `src/model_config.rs`
- Text requests: `src/api/text/translate/mod.rs`, `src/api/text/refine/mod.rs`
- Vision/audio/Live requests: `src/api/`, `src/overlay/`
- Preset defaults: `src/config/preset/defaults/`
- Android generated-catalog tasks: `mobile/shared/build.gradle.kts`, `mobile/androidApp/build.gradle.kts`

Report manifest changes, generated impact, migrations/defaults, capability logic, and verification.
