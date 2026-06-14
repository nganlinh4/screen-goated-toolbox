# Gemini Voice Catalog Parity

## Canonical Source
- Windows Gemini voice + instruction-language catalog: [src/config/tts_catalog_gemini.rs](../../src/config/tts_catalog_gemini.rs)
- Android Gemini voice + instruction-language catalog: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/model/GlobalTtsSettings.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/model/GlobalTtsSettings.kt)

## Behavior Contract
- Windows (Rust) is the single canonical owner of the Gemini voice and Gemini instruction-language catalogs.
- The shared fixture is the cross-platform source of truth that both platforms assert against; neither platform may edit its catalog without updating the fixture.
- Gemini voices:
  - 30 voices total, each with a name and a gender (`Male` or `Female`).
  - Rust stores them as a single ordered `(name, gender)` list (`GEMINI_VOICES`), alphabetical by name.
  - Android splits the same set into `MobileTtsCatalog.maleVoices` (16) and `MobileTtsCatalog.femaleVoices` (14). The union of those lists must reconstruct the fixture's voices exactly by name + gender.
- Gemini instruction languages:
  - 66 languages total, each with a code and a display name.
  - Rust stores them as `SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES`, ordered by code.
  - Android stores the same ordered list as `MobileTtsCatalog.conditionLanguages`. Codes, names, and order must match the fixture exactly.

## Platform Entry Points
- Android: TTS settings modal (Gemini Live method) — voice picker (male/female lists) and instruction-language picker (`conditionLanguages`).
- Windows: global TTS settings — Gemini voice selection and per-language instruction conditions.

## Deliberate Deviation
- Android is Gemini-only for this catalog. The Windows open-weight / local TTS engines (Kokoro, Supertonic, and the other Windows-local methods) and their voices are intentionally excluded on Android and are not part of this fixture. This exclusion is also captured in [parity-fixtures/tts-runtime/queue-semantics.json](../../parity-fixtures/tts-runtime/queue-semantics.json) (`android_open_weight_methods_are_feature_excluded`).
- Android organizes voices into separate male/female lists for the picker UI, whereas Windows keeps a single tagged list. Both encode the identical `(name, gender)` set, so this is a presentation-only split, not a catalog deviation.

## Fixtures
- Catalog fixture: [parity-fixtures/gemini-voice-catalog/catalog.json](../../parity-fixtures/gemini-voice-catalog/catalog.json)
  - `voices`: 30 `{ name, gender }` entries in canonical Rust order.
  - `instructionLanguages`: 66 `{ code, name }` entries in canonical Rust order.
- Rust assertion: `gemini_voices_match_parity_fixture` and `gemini_instruction_languages_match_parity_fixture` in [src/config/tts_catalog_gemini.rs](../../src/config/tts_catalog_gemini.rs).
- Android assertion: [mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiVoiceCatalogParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiVoiceCatalogParityTest.kt).
