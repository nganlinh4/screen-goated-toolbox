# Translation Gummy Parity

## Canonical Source
- Windows launcher and runtime: [src/overlay/translation_gummy/mod.rs](../../src/overlay/translation_gummy/mod.rs), [src/overlay/translation_gummy/runtime.rs](../../src/overlay/translation_gummy/runtime.rs)
- Windows persisted settings: [src/config/types/translation_gummy.rs](../../src/config/types/translation_gummy.rs), [src/config/config.rs](../../src/config/config.rs)
- Android runtime and foreground service: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyRuntime.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyRuntime.kt), [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyService.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy/TranslationGummyService.kt)

## Behavior Contract
- The feature is a dedicated Translation Gummy mini app backed only by `gemini-3.1-flash-live-preview`.
- The saved config contains two language profiles:
  - `language` is required
  - `accent` is optional
  - `tone` is optional
- Gemini Live model and Gemini voice for this feature come from the global TTS settings on both platforms, even when the active global TTS method is Edge TTS or Google Translate.
- Translation Gummy output volume is feature-local:
  - default is `100`
  - accepted range is `0..100`
  - UI increments are 5 percent
  - mute sets volume to `0`
  - unmute restores the previous nonzero value, falling back to `100`
  - changing volume affects playback scaling only and does not restart the Gemini Live session
- The system instruction is built from the same contract on both platforms:
  - identify which of the two configured languages the user spoke
  - answer only in the other configured language
  - do not prepend or append extra explanation
- Live sessions request:
  - `responseModalities = ["AUDIO"]`
  - `thinkingBudget = 0`
  - `inputAudioTranscription`
  - `outputAudioTranscription`
  - `realtimeInputConfig.automaticActivityDetection.startOfSpeechSensitivity = START_SENSITIVITY_HIGH`
  - `realtimeInputConfig.automaticActivityDetection.endOfSpeechSensitivity = END_SENSITIVITY_HIGH`
  - `realtimeInputConfig.automaticActivityDetection.prefixPaddingMs = 80`
  - `realtimeInputConfig.automaticActivityDetection.silenceDurationMs = 320`
  - `realtimeInputConfig.activityHandling = START_OF_ACTIVITY_INTERRUPTS`
  - `realtimeInputConfig.turnCoverage = TURN_INCLUDES_ONLY_ACTIVITY`
- Local mic streaming behavior is shared:
  - open a user turn only when local RMS reaches the speech threshold
  - keep a short pre-roll buffer so the start of the utterance is not clipped
  - keep trailing low-energy audio for a short grace window after speech
  - send `audioStreamEnd` after local end-of-speech silence so the server does not wait on an indefinitely open stream
- Valid saved config auto-starts the session when the surface opens.
- A localized onboarding guide must appear while `guide_seen` is false:
  - the popup title is the localized translation gummy title
  - the popup body uses the localized `translation_gummy_guide` text
  - dismissing the popup persists `guide_seen = true` and suppresses future auto-show, matching Windows `dismiss_guide`
- `Apply` always commits the current draft and forces a fresh session restart when the draft is valid.
- Applying a draft must preserve the existing transcript history and insert the same session separator behavior Windows uses before the restarted session; it must not clear the rolling transcript.
- The secondary control is a real `Start` / `Stop` toggle:
  - `Stop` ends the current session immediately
  - `Start` restarts the last applied valid config without requiring a new edit
- Socket loss triggers automatic reconnect with the same saved config.
- Transcript behavior is shared:
  - input transcription rows represent what the app heard
  - output transcription rows represent what the model spoke
  - partial rows are updated in place until the turn completes
  - new transcript rows appear centered first, then slide to the left or right bubble position once the language side is resolved
  - the transcript surface follows the newest row as new pills appear
- The bottom visualizer is driven by connection readiness state, not by a decorative timer.

## Platform Entry Points
- Android entry:
  - Apps tab
  - fourth carousel card
  - native Compose screen
  - foreground service keeps the session alive in background
- Windows entry:
  - footer button beside Pointer Gallery
  - optional global hotkey opens the window and starts the session
  - wry WebView child window is the feature surface

## Deliberate Deviation
- Windows is the canonical config and hotkey owner.
- Android does not expose a feature-specific hotkey.
- Android uses native Compose instead of a shared WebView surface because this feature was explicitly requested to follow the downloader-style Kotlin-native app pattern rather than the Windows-web parity pattern.
- Windows closes and stops the session when the mini app window is closed.
- Android places the Translation Gummy volume slider in the TTS settings modal opened from the Translation Gummy gear instead of the Windows header hover popup, because touch has no hover state and the user requested this modal placement.

## VAD + Setup Constant Lock
- The Gemini Live VAD numerics and session setup-payload constants are locked by the shared fixture [parity-fixtures/translation-gummy/vad-contract.json](../../parity-fixtures/translation-gummy/vad-contract.json). Rust is canonical.
- Canonical (Rust) values, from [src/overlay/translation_gummy/runtime.rs](../../src/overlay/translation_gummy/runtime.rs):
  - `vad.speechRms = 0.015` (`LOCAL_INPUT_SPEECH_RMS`)
  - `vad.trailingAudioMs = 180` (`LOCAL_INPUT_TRAILING_AUDIO_MS`)
  - `vad.endSilenceMs = 420` (`LOCAL_INPUT_END_SILENCE_MS`)
  - `vad.prerollChunks = 2` — Windows stores this as samples (`LOCAL_INPUT_PREROLL_SAMPLES = 3200 = 2 * CHUNK_SAMPLES(1600)`)
  - `setup.startSensitivity = START_SENSITIVITY_HIGH`, `setup.endSensitivity = END_SENSITIVITY_HIGH`
  - `setup.prefixPaddingMs = 80`, `setup.silenceDurationMs = 320`
  - `setup.thinkingBudget = 0`, `setup.mediaResolution = MEDIA_RESOLUTION_LOW`
  - `setup.activityHandling = START_OF_ACTIVITY_INTERRUPTS`, `setup.turnCoverage = TURN_INCLUDES_ONLY_ACTIVITY`
- Per-platform assertions:
  - Windows (Rust): `vad_contract_tests` in [src/overlay/translation_gummy/runtime.rs](../../src/overlay/translation_gummy/runtime.rs) `include_str!`s the fixture and asserts each `LOCAL_INPUT_*` constant + the rebuilt setup payload (`build_setup_payload`). Pre-roll is asserted in samples: `LOCAL_INPUT_PREROLL_SAMPLES == prerollChunks * CHUNK_SAMPLES`.
  - Android (Kotlin): [TranslationGummyVadContractTest](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/TranslationGummyVadContractTest.kt) loads the same fixture and asserts `TranslationGummyRuntime.LOCAL_INPUT_*` constants + the `buildTranslationGummySetupPayload` output. Pre-roll is asserted as chunks (`LOCAL_INPUT_PREROLL_CHUNKS == prerollChunks`), not raw samples, because the Android audio chunk size is device-dependent.

## Fixtures
- Prompt fixture: [parity-fixtures/translation-gummy/prompt-contract.json](../../parity-fixtures/translation-gummy/prompt-contract.json)
- Socket protocol fixture: [parity-fixtures/translation-gummy/socket-protocol.json](../../parity-fixtures/translation-gummy/socket-protocol.json)
- VAD + setup constant fixture: [parity-fixtures/translation-gummy/vad-contract.json](../../parity-fixtures/translation-gummy/vad-contract.json)
- Onboarding fixture: [parity-fixtures/translation-gummy/onboarding-contract.json](../../parity-fixtures/translation-gummy/onboarding-contract.json)
- Volume fixture: [parity-fixtures/translation-gummy/volume-control.json](../../parity-fixtures/translation-gummy/volume-control.json)
- State fixture: [parity-fixtures/translation-gummy/state-contract.json](../../parity-fixtures/translation-gummy/state-contract.json)
