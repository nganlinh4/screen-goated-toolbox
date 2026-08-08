# Model Catalog Presentation Parity

## Canonical Source

- Windows identity/data owner: [catalog/model_catalog.json](../../catalog/model_catalog.json)
- Naming and performance contract: [catalog/README.md](../../catalog/README.md)
- Windows presentation helper: [src/gui/model_performance.rs](../../src/gui/model_performance.rs)
- Android descriptor/presentation: [PresetModelCatalog.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetModelCatalog.kt), [ModelPerformancePrefix.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/ModelPerformancePrefix.kt)
- Screen Recorder model transport/presentation: [subtitle types](../../src/overlay/screen_record/ipc/subtitles/types.rs), [PanelSelect](../../screen-record/src/components/ui/PanelSelect.tsx)

## Behavior Contract

- Built-in model IDs, endpoint profiles, localized names, quotas, search
  capability, default search-tool behavior, reasoning policy, intelligence
  tiers, and typical latency come only from the shared catalog.
- Every expanded built-in model row and every authored priority-chain row
  starts with fixed-width intelligence-stat and latency columns, followed by
  its provider icon and model text. The latency column uses tabular numerals
  and end alignment, so `3s`, `3.3s`, and `20s` share the same right edge. The
  intelligence and latency columns are one compact prefix widget with only the
  fixture-owned inter-column gap; platform default item spacing must not be
  inserted between them.
- One of six stat icons represents `intelligence_tier`. Typical latency follows the
  shared one-decimal/trimmed-zero seconds format.
- Custom and discovered models retain aligned columns and show em dashes because
  no benchmark metadata exists.
- General collapsed selectors may use the short localized name only. Priority
  rows remain visible comparison surfaces even while their selector is
  collapsed, so they always show the aligned performance prefix. The same
  prefix is required in lists, menus, locked catalog rows, usage rows, and
  recorder model options.
- Localized names preserve the provider prefix and remain unique within that
  provider-prefix group.
- OpenRouter uses the durable `openrouter-` ID namespace and the localized
  `O` prefix. Its upstream model slug stays only in `full_name`.
- The same provider + API `full_name` has the same localized base name in every
  modality. A catalog-owned presentation variant may append a localized suffix
  only to distinguish behavioral rows sharing that endpoint. The native-Live
  input-transcription rows append `(Chép)` in Vietnamese; their custom-prompt
  siblings retain the base name. Identically named models served by different
  providers retain provider-specific names, quotas, and request policy.
- Lists sort globally by latency regardless of provider, with durable model ID
  as the tie-breaker. Runtime priority/retry chains retain their authored order.
- A benchmark-qualified built-in endpoint becomes selectable
  on Windows and Android from the same catalog revision; neither platform may
  hide it behind a platform-local allowlist or duplicate its fallback placement.
- Quota labels mean daily request count and use only the localized daily-count
  form or localized Unlimited form.
- `supports_search` is provider capability metadata used for explicit
  search-path compatibility. It never draws the search marker by itself.
  A model row shows the marker only when its endpoint profile sets
  `search_tool_enabled_by_default`, meaning selecting that model in the normal
  model path actually invokes search. Quota-bearing Google grounding remains
  explicit-feature-only; Groq Compound is marked because its normal request
  enables provider-managed web tools.

## Failure And Recovery

- Invalid built-in tiers, missing latency, invalid ID grammar, wrong localized
  prefix, forbidden category/lifecycle naming, duplicate rendered names,
  malformed or unreferenced presentation variants, malformed quota triplets, a
  default search tool without provider support, or provider-incompatible
  reasoning policies fail catalog generation.
- Unknown persisted IDs do not alias or migrate. Preset loading replaces them
  with the canonical default for the block modality.
- A model without successful benchmark evidence may remain selectable, but its
  reliability warning and retry position must not imply measured stability.
- Catalog decisions use only the newest complete protocol-compatible benchmark
  row for each model, suite, endpoint, and reasoning policy. Older runs remain
  auditable history but never contribute to current latency, accuracy, or
  reliability values.

## Fixtures

- Shared fixture: [parity-fixtures/model-catalog/presentation.json](../../parity-fixtures/model-catalog/presentation.json)
- Windows tests: model catalog lifecycle/presentation tests.
- Android tests: `ModelCatalogPresentationParityTest`.

## Deviations

- Screen Recorder uses fixed flex columns inside its shared select component.
- Windows uses egui fixed-size labels and Android uses fixed-width Compose text.
  The visual primitives differ; ordering, values, and formatting do not.
