# API Key Notification Parity

## Canonical Source
- Windows shared error normalization: [src/overlay/utils/error_messages.rs](../../src/overlay/utils/error_messages.rs)
- Windows overlay result execution: [src/overlay/process/chain/execution.rs](../../src/overlay/process/chain/execution.rs), [src/overlay/result/mod.rs](../../src/overlay/result/mod.rs)
- Windows Gemini Live LLM path: [src/api/gemini_live/mod.rs](../../src/api/gemini_live/mod.rs), [src/api/gemini_live/worker.rs](../../src/api/gemini_live/worker.rs)
- Windows TTS worker: [src/api/tts/worker/mod.rs](../../src/api/tts/worker/mod.rs)
- Android app toast bus: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/AppToastBus.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/AppToastBus.kt)
- Android preset graph/audio/runtime surfaces: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetGraphExecutor.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetGraphExecutor.kt), [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetAudioCaptureSession.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetAudioCaptureSession.kt), [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/AndroidTtsRuntimeService.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/AndroidTtsRuntimeService.kt)
- Android translation gummy startup: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyService.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyService.kt)

## Behavior Contract
- Any `NO_API_KEY:*`, `INVALID_API_KEY`, or `INVALID_API_KEY:*` failure must be surfaced as a global user notice on the active platform.
- The notice must appear even if the initiating surface does not have a visible overlay or screen-level error card.
- Windows uses the shared overlay badge toast surface for the notice.
- Android uses a single app-level toast bus for the notice.
- Local in-window or in-screen error state may still render, but it must not be the only user-visible signal for an API-key failure.
- The toast must preserve the provider name when available; preset provider clients should emit provider-bearing invalid-key errors instead of collapsing 401/403 responses to generic `INVALID_API_KEY`.

## Fixtures
- Shared triggers: [parity-fixtures/api-key-notifications/triggers.json](../../parity-fixtures/api-key-notifications/triggers.json)

## Deviations
- None.
