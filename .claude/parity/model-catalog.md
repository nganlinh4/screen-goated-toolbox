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
  as the tie-breaker. Runtime priority/retry chains retain the relative order of
  authored rows. Eligible unpinned live-feed rows may enter before a slower
  authored fallback without truncating user-authored rows, but only from position
  3 onward: the primary and immediate local fallback stay ahead of automatic
  remote offers. An explicit user pin may retain its authored anchor.
- Windows and Android expose one persisted adaptive-model toggle beside each priority-chain
  title. While enabled, its formula-ranked live-feed entries render as ordinary
  selectable, draggable, removable chain rows. Moving a live row pins that row at
  its authored anchor; removing it records a chain-local exclusion. These narrow
  overrides and edits to other rows preserve Live, so every unpinned, non-excluded
  offer continues to receive availability and formula-order updates. Changing a
  live row excludes its old identity and pins its live replacement. Restoring the
  chain defaults clears its pins and exclusions. A manual edit that leaves no
  live-feed row in the chain disables Live; Add stays available. Live-off chains
  retain their authored manual order. Both platforms consume the same signed
  availability document, apply the same endpoint controls, and reject invalid,
  stale, incompatible, or unsigned documents without replacing the last valid
  cached feed.
- While Live owns a priority row, its latency prefix shows the current signed-feed
  p50 used by the adaptive formula. Turning Live off restores the durable catalog
  benchmark label; ranking and its visible explanation must never use different
  latency sources.
- While Live is enabled and a verified feed plus usable credential are present,
  that signed offer set owns NVIDIA operational availability. Feed-absent NVIDIA
  rows are removed from the effective chain and generic fallback search; newly
  offered rows become immediately selectable and routable. Reviewed withdrawn
  endpoints remain the only catalog-level veto.
- Every selector projects NVIDIA inventory from the newest verified feed whether
  adaptive ordering is on or off. The Live toggle controls formula ownership of
  priority order; it never makes a stale compiled NVIDIA endpoint look available.
- Signed-feed endpoint identity includes provider, exact endpoint, and modality.
  A cataloged text row must not capture a vision offer for the same endpoint (or
  vice versa); the unmatched capability is projected as a compactly named
  discovered row with its feed latency on both platforms.
- The image `10` and text `12` counts are shipped-default preparation targets,
  never UI, persistence, or runtime limits. Users may add any number of rows.
- The chosen-model sentinel is numbered `0`, editable retry rows are numbered
  continuously from `1`, and the automatic-fallback sentinel receives the next
  number after the final visible row.
- A benchmark-qualified built-in endpoint becomes selectable
  on Windows and Android from the same catalog revision; neither platform may
  hide it behind a platform-local allowlist or duplicate its fallback placement.
- Default retry-chain heads use the newest reviewed complete-result evidence.
  Once a model clears the general-task quality and reliability floor, lower
  end-to-end latency wins; a newer endpoint may replace its predecessor at the
  head only after the production request path shows no output-restatement defect.
  A clean but less-proven successor occupies the second slot until that evidence
  is complete.
- Every model-selection surface, including the Android node editor, observes
  live-feed revisions and applies the shared provider-enabled predicate. A
  newly offered NVIDIA endpoint therefore appears without reopening the editor,
  while disabling NVIDIA removes it from selectors without deleting persisted
  chain choices.
- Architecture never implies general capability. Translation-only, search-only,
  embedding, and other dedicated endpoints remain outside generic Text-to-Text
  priority chains even when implemented as LLMs and even when they pass their
  dedicated task suite.
- Gemini 3.5 Transcribe owns two distinct Audio rows: the `gemini-live` row uses
  the dedicated WebSocket endpoint for continuous capture, while the `google`
  row uses unary audio-file transcription. The continuous-audio preset defaults
  to the dedicated Live row. One-shot transcription remains on Whisper until a
  comparable reviewed audio benchmark supports a default change.
- Lifecycle-disabled modality rows retain their durable catalog identity but
  are excluded from selectable generated catalogs and retry chains on both
  platforms. Ordinary vision request profiles cover enabled rows only.
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
- A verified feed cache is replaced atomically when the filesystem supports it;
  a same-directory replace is the compatibility fallback. Lack of atomic-move
  support must not prevent a valid signed refresh from becoming active.
- Removing an unavailable endpoint removes its profile and model rows atomically;
  recommended chains may replace it only with another enabled, benchmarked row
  already owned by the shared catalog.
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
- Android uses native Compose controls while Windows uses egui controls; the
  signed feed, adaptive merge, pins, exclusions, and persistence semantics are
  shared behavior.
- A signed feed names both its wire schema and the operational-availability
  contract that admitted its candidates. Windows rejects feeds using obsolete
  availability semantics; signature validity alone does not make stale evidence
  current. Preset samples may exercise the monitor, but no preset-specific
  result can admit or remove a model from a whole Text-to-Text or Image-to-Text
  catalog. Durable quality remains owned by the catalog benchmark.
