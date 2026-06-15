# Zipformer Streaming-ASR Catalog Parity

## Canonical Source
- Windows (canonical): [src/api/realtime_audio/sherpa_onnx/mod.rs](../../src/api/realtime_audio/sherpa_onnx/mod.rs) — `ZipformerLanguage`
- Android: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/moonshine/ZipformerLanguage.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/moonshine/ZipformerLanguage.kt)

## Behavior Contract
Both platforms expose the same 8 streaming-Zipformer ASR models (EN, KO, ZH, FR,
DE, ES, RU, and the All-8 multilingual model). The Windows enum is canonical; the
Android enum must encode the identical per-language download/runtime data so a
model that installs on one platform installs and runs on the other. The shared
fixture locks the fields that MUST stay in sync:
- `code` — stable language id (`en`/`ko`/`zh`/`fr`/`de`/`es`/`ru`/`all-8`).
- `modelName` — the on-disk model directory name.
- `downloadBaseUrl` — HuggingFace/ModelScope base URL the model files download from.
- `hasNativePunctuation` — whether the model emits punctuation (drives the
  offline-ASR commit machine's Case 1/2 vs Case 3 — see
  [offline-asr-stream.md](offline-asr-stream.md)).
- `modelFiles` — the exact list of files to download per model.

Neither platform may change any of these without updating the fixture (which makes
the other platform's test fail until it is reconciled).

## Deliberate Deviations
- **All-8 `displayName`.** Windows uses `"AR,EN,ID,JA,RU,TH,VI,ZH"`; Android uses
  `"AR, EN, ID, JA, RU, TH, VI, ZH"` (spaces for readability). Presentation-only;
  not asserted by the fixture.
- **`sherpaModelType` (⚠️ needs on-device validation).** Windows sets `"zipformer2"`
  for the Kroko EN/FR/DE/ES models and auto-detects (`""`) for the rest; Android
  currently leaves all entries at the `""` auto-detect default. This value is passed
  to the sherpa-onnx recognizer config on both platforms, so it is a live behavioral
  setting, not dead metadata. Whether Android needs the explicit `"zipformer2"` hint
  depends on whether the Kroko ONNX files embed model-type metadata (if they do,
  auto-detect is sufficient). Reconciling requires running a Kroko model on an
  Android device and confirming it loads/decodes; until then the value is excluded
  from the fixture and the divergence is documented here.

## Fixtures
- Catalog fixture: [parity-fixtures/zipformer-catalog/catalog.json](../../parity-fixtures/zipformer-catalog/catalog.json) — 8 `{ code, modelName, downloadBaseUrl, hasNativePunctuation, modelFiles }` entries in Windows order.
- Rust assertion: `windows_zipformer_catalog_matches_parity_fixture` in [src/api/realtime_audio/sherpa_onnx/mod.rs](../../src/api/realtime_audio/sherpa_onnx/mod.rs).
- Android assertion: [mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/ZipformerCatalogParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/ZipformerCatalogParityTest.kt).

## Changing this catalog
Add/edit/remove a model on the Windows canonical enum, mirror it on Android, and
update `catalog.json` so both suites assert the new contract. If the change is
runtime-behavioral (model type, decoding config), validate it on-device before
relying on it.
