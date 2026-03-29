# Bilingual Relay Parity

## Canonical Source
- Windows launcher and runtime: [src/overlay/bilingual_relay/mod.rs](../../src/overlay/bilingual_relay/mod.rs), [src/overlay/bilingual_relay/runtime.rs](../../src/overlay/bilingual_relay/runtime.rs)
- Windows persisted settings: [src/config/types/bilingual_relay.rs](../../src/config/types/bilingual_relay.rs), [src/config/config.rs](../../src/config/config.rs)
- Android runtime and foreground service: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/bilingualrelay/BilingualRelayRuntime.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/bilingualrelay/BilingualRelayRuntime.kt), [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/bilingualrelay/BilingualRelayService.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/bilingualrelay/BilingualRelayService.kt)

## Behavior Contract
- The feature is a dedicated bilingual live relay mini app backed only by `gemini-3.1-flash-live-preview`.
- The saved config contains two language profiles:
  - `language` is required
  - `accent` is optional
  - `tone` is optional
- Gemini Live model and Gemini voice for this feature come from the global TTS settings on both platforms, even when the active global TTS method is Edge TTS or Google Translate.
- The system instruction is built from the same contract on both platforms:
  - identify which of the two configured languages the user spoke
  - answer only in the other configured language
  - do not prepend or append extra explanation
- Live sessions request:
  - `responseModalities = ["AUDIO"]`
  - `thinkingBudget = 0`
  - `inputAudioTranscription`
  - `outputAudioTranscription`
- Valid saved config auto-starts the session when the surface opens.
- `Apply` always commits the current draft and forces a fresh session restart when the draft is valid.
- The secondary control is a real `Start` / `Stop` toggle:
  - `Stop` ends the current session immediately
  - `Start` restarts the last applied valid config without requiring a new edit
- Socket loss triggers automatic reconnect with the same saved config.
- Transcript behavior is shared:
  - input transcription rows represent what the app heard
  - output transcription rows represent what the model spoke
  - partial rows are updated in place until the turn completes
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

## Fixtures
- Prompt fixture: [parity-fixtures/bilingual-relay/prompt-contract.json](../../parity-fixtures/bilingual-relay/prompt-contract.json)
