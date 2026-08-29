---
name: manage-model-catalog
description: Add, edit, rename, reorder, or remove models through the canonical catalog and every generated/runtime consumer.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Manage Model Catalog

## Source of Truth

`catalog/model_catalog.json` owns model IDs, exact API endpoint profiles,
defaults, aliases, display order, capabilities, quotas, reasoning policy, and
ordinary vision request profiles, and shared provider metadata. Never start by
hardcoding a model in a UI or platform runtime.

`catalog/README.md` owns the durable internal-ID namespace, localized-name
prefixes, performance metadata, migration policy, and priority policy. Apply
those rules to every catalog change; do not invent a feature-local convention.

## Workflow

1. Confirm operation, model type, provider, internal ID, API `full_name`,
   endpoint profile, enabled state, and default/fallback impact. Reuse an
   existing `model_profiles["<provider>:<full_name>"]` for another modality.
   Keep one localized base name per endpoint; use a catalog-owned
   `presentation_variant` only when behavioral rows sharing that endpoint need
   a localized disambiguating suffix.
2. Run the API-first discovery report described by
   `.claude/skills/update-model-catalog/SKILL.md`, then check current official
   model/deprecation documentation for every reported ambiguity. Browser control
   is a fallback, not the routine inventory mechanism.
3. Search both identifiers and sibling-family capability logic:

```powershell
rg -n '<internal-id>|<api-model>|supports_thinking|supports_search|model_is_non_llm|get_model_by_id' catalog src mobile scripts parity-fixtures .claude
```

4. Before changing performance metadata or retry priority, collect a complete
   protocol-compatible live run under
   `tests/catalog-benchmark/history-policy.json`. Use only the latest complete
   row for each model/suite: latest-run median latency and reliability including
   every attempt and error. For vision, use the OCR row's representative small-image
   `catalog_latency_ms`; coordinate rows are control-task evidence, while the
   all-case median and P95 retain large-image stress evidence.
   Focused recovery reports may be merged into the same logical run before that
   complete report is registered; never register recovery fragments
   independently. Every manual-review suite needs complete structured human review. Increment
   `benchmark_protocol_version` before collecting results when benchmark
   scoring or request semantics change.
5. Edit the manifest and every relevant manifest section: endpoint profile,
   constants, defaults, provider defaults, priority chains, aliases, TTS/Live
   lists, non-LLM sets, and `vision_request_profiles`. Every enabled ordinary
   LLM vision endpoint must explicitly declare input order, media resolution,
   sampling, optional output-token ceiling, and structured-output wire policy.
   `supports_search`,
   `search_tool_enabled_by_default`, localized daily-request quota,
   intelligence tier, and ordinary reasoning policy are explicit profile data;
   do not add model-name heuristics. Search capability alone never authorizes a
   tool or marker: inspect the production payload and set default tool behavior
   independently.
6. Audit feature-specific request logic in `src/api/`, `src/overlay/`, and
   Android clients. Verify the production request against the exact generated
   vision profile; do not add a model-name heuristic. A catalog entry does not
   automatically make a wire protocol compatible.
7. Update presets, parity fixtures, and tests that use the internal ID. When
   preset defaults, priority chains, or provider defaults change, verify the
   one-time Windows recommendation marker snapshots and applies all three
   groups. Applying provider recommendations is additive: enable catalog
   recommendations without disabling any extra provider. The catalog
   intentionally has no permanent model-ID migration table; unknown saved IDs
   fall back by modality.
8. Regenerate Android outputs. Gradle does this during normal builds; for direct inspection:

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

When `build_support/model_catalog.rs` changes the generated Rust output shape or
constant mappings, increment `MODEL_CATALOG_GENERATOR_SCHEMA` in `build.rs` in
the same change. This invalidates a cached build-script executable instead of
letting it regenerate the catalog with old generator logic. Validate through the
managed warm cache with:

```powershell
.\run-dev.ps1 -SkipFrontendBuild -SkipCacheMaintenance -CargoCommand check
```

9. For removal or an intentional namespace rewrite, prove no dangling active
   references remain with `rg`.
10. Run focused tests, then repository validation from `AGENTS.md`. For Android catalog changes, run the relevant Gradle compile/unit tests from `mobile/README.md`.

## High-Risk Owners

- Windows catalog wrapper: `src/model_config.rs`
- Text requests: `src/api/text/translate/mod.rs`, `src/api/text/refine/mod.rs`
- Vision/audio/Live requests: `src/api/`, `src/overlay/`
- Preset defaults: `src/config/preset/defaults/`
- Android generated-catalog tasks: `mobile/shared/build.gradle.kts`, `mobile/androidApp/build.gradle.kts`

Report manifest changes, generated impact, migrations/defaults, capability logic, and verification.
