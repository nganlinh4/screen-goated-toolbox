# Model Catalog Presentation Parity

## Canonical Source

- Windows identity/data owner: [catalog/model_catalog.json](../../catalog/model_catalog.json)
- Naming and performance contract: [catalog/README.md](../../catalog/README.md)
- Windows presentation helper: [src/gui/model_performance.rs](../../src/gui/model_performance.rs)
- Android descriptor/presentation: [PresetModelCatalog.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetModelCatalog.kt), [ModelPerformancePrefix.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/ModelPerformancePrefix.kt)
- Screen Recorder model transport/presentation: [subtitle types](../../src/overlay/screen_record/ipc/subtitles/types.rs), [PanelSelect](../../screen-record/src/components/ui/PanelSelect.tsx)

## Behavior Contract

- Built-in model IDs, localized names, quality tiers, and typical latency come
  only from the shared catalog.
- Every expanded built-in model row starts with fixed-width brain and latency
  columns, followed by its provider icon and model text.
- One to five brain icons represent `quality_tier`. Typical latency follows the
  shared one-decimal/trimmed-zero seconds format.
- Custom and discovered models retain aligned columns and show em dashes because
  no benchmark metadata exists.
- Collapsed selectors may use the short localized name only; the aligned
  performance columns are required in lists, menus, locked catalog rows, usage
  rows, and recorder model options.
- Localized names preserve the provider prefix and remain unique within that
  provider-prefix group.

## Failure And Recovery

- Invalid built-in tiers, missing latency, invalid ID grammar, wrong localized
  prefix, or duplicate localized names fail catalog generation.
- Unknown persisted IDs do not alias or migrate. Preset loading replaces them
  with the canonical default for the block modality.
- A model without successful benchmark evidence may remain selectable, but its
  reliability warning and retry position must not imply measured stability.

## Fixtures

- Shared fixture: [parity-fixtures/model-catalog/presentation.json](../../parity-fixtures/model-catalog/presentation.json)
- Windows tests: model catalog lifecycle/presentation tests.
- Android tests: `ModelCatalogPresentationParityTest`.

## Deviations

- Screen Recorder uses CSS grid columns inside its shared select component.
- Windows uses egui fixed-size labels and Android uses fixed-width Compose text.
  The visual primitives differ; ordering, values, and formatting do not.
