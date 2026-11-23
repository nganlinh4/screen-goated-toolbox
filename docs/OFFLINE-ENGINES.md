# Offline Engines

On 2026-11-12 every offline speech model was removed from Windows and Android
(owner decision: online engines are better, and offline maintenance cost too
much). The extension points below stay, so a future offline engine can plug in
without rebuilding the framework. The removed implementations remain in git
history: check out commit `23471bf6` (the last commit that contains them) or run
`git log --diff-filter=D -- <path>` on any path listed below.

## Where a new engine plugs in

1. **Catalog entry** (`catalog/model_catalog.json`, rules in `catalog/README.md`)
   - Live Translate selector: add the id to `realtime_transcription_options.<platform>`,
     `realtime_transcription_labels`, `realtime_transcription_aliases` and
     `realtime_transcription_engines` (`on_device_engine`, `platforms`). Add the
     engine family to `PLATFORM_ENGINES` in `scripts/model_catalog_realtime.py`.
     List it in `realtime_transcription_requires_source_language` if it needs an
     explicit source language; the Windows source-language pickers then appear
     for it automatically.
   - Preset audio model: add a `models` row, its `model_profiles` entry, and the
     id in `non_llm_ids`. A new provider needs its name and id prefixes in
     `build_support/model_catalog_validation.rs` and
     `scripts/model_catalog_validation.py` (local runtimes use `L` / `local-`).
   - Regenerate with the official generator (the `manage-model-catalog` skill).
2. **Component kind** (`src/component_registry/`)
   - Declare the component in `component-catalog/catalog-v1.json` (kind, owned
     paths, dependencies) and its update policy in
     `component-delivery/update-catalog-v1.sources.json`.
   - Model weights: add a spec to `PACKAGES` in `scripts/package_windows_models.py`;
     `scripts/generate_windows_model_delivery.py` writes `model-delivery/windows-v1.json`
     (now `"models": []`). Its Rust consumer (`src/component_registry/models/`,
     `src/api/realtime_audio/model_component_assets.rs`) was removed with its last
     model; restore it from `23471bf6`, plus a `ModelKind` variant.
   - Native DLL runtime: follow `component_registry/local_asr.rs`, which still
     delivers the ONNX/DirectML runtime from
     `component-delivery/windows/local-asr-v1.json` (built by
     `build_support/local_asr_delivery.rs`, packaged by
     `scripts/build-local-asr-packs.ps1`).
   - A one-shot install id belongs in `interrupted_installs.rs`.
3. **Runtime launch**
   - Prefer an isolated worker process with its own protocol crate under
     `native/` (see `screen_translate_worker` and `computer_control_engine`),
     launched from a leased, verified component.
   - Live Translate dispatch: `src/api/realtime_audio/transcription.rs`
     (`TranscriptionMethod`); show model downloads in the overlay through
     `src/overlay/realtime_webview/download.rs` `begin`. Preset audio:
     `src/api/audio/transcription/audio_block.rs`. Screen Record subtitles:
     `src/overlay/screen_record/ipc/subtitles/providers/` (`SubtitleGenerationMethod`)
     and `screen-record/src/hooks/subtitleGenerationStorage.ts`.
4. **Settings row**
   - Downloaded Tools (`src/gui/settings_ui/global/downloaded_tools/`):
     `component_row::render_component_row` for runtimes and
     `model_card::render_model_row` for model packs. Shared native runtimes live
     in the "Shared runtimes" card (`ai_runtime.rs`).
   - Folders left by a retired engine go in `component_registry/legacy_offline.rs`,
     which drives the "Legacy offline models" cleanup row.
5. **Android module**: see the Android section below.

## Removed: Windows offline ASR (Qwen3-ASR 0.6B/1.7B, Parakeet, Zipformer)

| Area | Paths |
| --- | --- |
| Native crates | `native/local_asr_worker`, `native/local_asr_protocol`, `native/qwen3_runtime`, `native/qwen3_reference_sidecar` |
| Vendored sources | `third_party/qwen3-asr-rs`, `third_party/parakeet-rs`; ignored `libs/tch`, `libs/torch-sys` |
| Build and packaging | `build_support/qwen_runtime_delivery.rs`, `build_support/qwen_runtime_paths.rs`, `component-delivery/windows/qwen-runtime-v1.json`, `component-notices/qwen3-cuda-runtime`, `.github/workflows/qwen3-*.yml`, `scripts/*qwen3*`, `scripts/qwen3-dependency-patches.json`, `tests/qwen_runtime_paths.rs` |
| Registry | `src/component_registry/qwen_runtime*`; the worker half of `src/component_registry/local_asr.rs`; `ModelKind::QwenSmall`/`QwenLarge` |
| Realtime and presets | `src/api/realtime_audio/{local_asr_worker,parakeet,parakeet_tdt_assets,qwen3,model_loader,offline_asr_commit,transcript_state}*`, the Zipformer part of `src/api/realtime_audio/sherpa_onnx/`, `record_and_stream_parakeet` in `src/api/audio/recording.rs` |
| Subtitles | `src/overlay/screen_record/ipc/subtitles/{qwen_local,parakeet_tdt}.rs`, `providers/{qwen,parakeet_tdt}.rs`; Screen Record methods `qwen-local-*`, `parakeet-tdt-0-6b-v3` |
| Settings | `downloaded_tools/zipformer.rs`, the Parakeet and Qwen3 cards in `model_sections.rs`, the worker row in `ai_runtime.rs` |
| Catalog | ids `parakeet`, `local-parakeet-120m-v1-audio`, `local-qwen-3-asr-600m-audio`, `local-qwen-3-asr-1-7b-audio`, `qwen3-asr-local`; Windows `zipformer` option |

Kept: the ONNX/DirectML runtime (Screen Translate uses it) under its historical
`local-asr` contract and script names. Saved selections of removed ids normalize
to the catalog default.

## Android

Plug-in points (all model-neutral code still has live users):
- Catalog: an `on_device_engine` entry whose `platforms` include `android`, plus the
  id in `realtime_transcription_options.android`. Until a runtime exists,
  `RealtimeModelIds.isUnavailableOnAndroid` keeps such a selector visible but
  disabled, and `requiresGeminiApiKey` never asks for a key for it.
- Module: a dynamic feature modeled on `mobile/feature_creation_runtime`
  (`settings.gradle.kts` include, `dynamicFeatures` in `androidApp/build.gradle.kts`,
  `allowedFeatureModules` in `androidApp/gradle/play-compliance.gradle.kts`).
  A Full-flavor download should reuse the downloader runtime's verified delivery.
- Settings row: a card in `ui/DownloadedToolsDialog.kt`. Folders a retired engine
  left behind go in `storage/LegacyOfflineModels.kt`, which drives the
  "Legacy offline models" row there.

Removed (Moonshine Tiny/Small/Medium, Zipformer, Parakeet placeholder):
`mobile/feature_asr_{moonshine,ort,sherpa}`, `mobile/native/`,
`mobile/androidApp/native/ort_proxy`, `mobile/androidApp/libs/*-runtime.zip`
and the sherpa-onnx AAR, `service/moonshine/`, `service/nativelibs/` (full, play
and main contracts), `LiveSessionRuntimeOfflineAsr.kt`,
`NativeEngineLeaseLoader.kt`, the overlay model-download modal and
transcription-language badge, shared `OfflineAsrStreamParity.kt` and
`ParakeetCommitTimeout.kt`, `mobile/scripts/{build-ort-runtime,build-sherpa-runtime,smoke-ort-runtime}.ps1`,
`package-moonshine-models.py`, `verify-moonshine-model-release.py`,
`verify_native_runtime_archives.py`, and the fixtures
`parity-fixtures/{offline-asr-stream,zipformer-catalog}`,
`parity-fixtures/phone-control/native-runtime-contract.json`,
`parity-fixtures/live-translate/parakeet-capitalization.json`.

## Removed: Windows offline TTS (Kokoro, Step Audio EditX, Magpie, VieNeu, Fish Speech, Voxtral, Supertonic 3)

| Area | Paths |
| --- | --- |
| Runtimes (in `23471bf6`) | `native/{magpie,step_audio,vieneu}_runtime`, `src/api/realtime_audio/{kokoro,magpie,step_audio,vieneu}_*`, `src/api/tts/worker/{worker_kokoro,worker_magpie,worker_step_audio,worker_vieneu,sidecar,native_support,worker_unavailable}.rs` |
| Never committed (lost) | `native/sgt_tts_runtime{,_py}`, `native/fish_speech_runtime`, `src/component_registry/audio_runtime*`, `build_support/audio_runtime_*`, `component-delivery/windows/audio-runtimes-v1.json` |
| UI | the Kokoro/Step Audio/Magpie/VieNeu rows in `tts_settings/` and `downloaded_tools/`, the playground Audio Edit tab and reference library, `config/types/tts/step_audio.rs` |
| Catalog | `tts_open_models` rows (the field stays, empty) |
| Supertonic 3, sherpa-onnx (in `23471bf6`) | `src/api/tts/worker/worker_supertonic.rs`, `src/api/realtime_audio/{supertonic_assets.rs,sherpa_onnx/}`, `downloaded_tools/model_sections.rs`, `tts_settings/providers.rs`, `SupertonicSettings`; ignored `third_party/sherpa-onnx-win` |
| Sherpa delivery (never committed, lost) | `component_registry/local_asr/sherpa.rs`, `build_support/sherpa_runtime_delivery.rs`, `component-delivery/windows/sherpa-runtime-v1.json`, `scripts/{build-sherpa-runtime-pack.ps1,package_sherpa_runtime.py}`, `downloaded_tools/sherpa_{runtime,status}.rs` |

Saved retired methods, Supertonic included, deserialize to Gemini Live
(`src/config/types/tts.rs` aliases); recorded reference clips stay on disk. A new
TTS engine plugs in at `TtsMethod` plus `TTS_PROVIDERS` (`src/config/tts_catalog.rs`),
dispatch in `src/api/tts/worker/mod.rs` (shared PCM plumbing in `open_weights.rs`),
a row in `tts_settings.rs`, and a `components/<id>` registry kind as above.
