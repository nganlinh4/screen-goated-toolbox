# Translation Gummy Parity

## Canonical Source

- Windows surface/runtime: [translation_gummy](../../src/overlay/translation_gummy)
- Windows settings: [translation_gummy.rs](../../src/config/types/translation_gummy.rs)
- Android feature: [translationgummy](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/translationgummy)
- Shared fixtures: [translation-gummy](../../parity-fixtures/translation-gummy)

## Contract

- Two profiles each require a language and may add accent/tone.
- Detect the spoken profile and output only the translation in the other profile.
- Use the globally selected Gemini Live model and voice, independent of the active global TTS method.
- Build setup per selected model. Gemini 2.5 uses `thinkingBudget`; Gemini 3.1 uses `thinkingLevel`. Do not force one family's field onto the other.
- Enable input/output transcription, interruption on new activity, context compression, and the VAD/setup values locked by `vad-contract.json`.
- Process every content part in a server event; Gemini 3.1 may combine audio and transcript parts.
- Local mic gating keeps preroll/trailing audio and sends `audioStreamEnd` after local end-of-speech.
- Valid saved config auto-starts. Apply persists a valid draft, preserves transcript history, inserts the session separator, and restarts.
- Start/Stop controls the last applied config. Socket loss reconnects without erasing transcript state.
- Partial transcript rows update in place; final rows resolve to the matching language side; the visualizer reflects connection/readiness state.
- Volume is feature-local (`0..100`, default 100, 5-point UI steps). Mute remembers the previous nonzero value. Volume changes playback only and do not restart the session.
- The onboarding guide appears until dismissal persists `guide_seen = true`.

## Entry and Deviation

- Android: second Apps carousel card, native Compose UI, foreground service, no feature hotkey.
- Windows: footer launcher and optional hotkey, Wry surface, session stops when the window closes.
- Android keeps volume settings in the touch-friendly TTS modal; Windows uses its desktop header control.

## Fixtures

- [prompt-contract.json](../../parity-fixtures/translation-gummy/prompt-contract.json)
- [socket-protocol.json](../../parity-fixtures/translation-gummy/socket-protocol.json)
- [vad-contract.json](../../parity-fixtures/translation-gummy/vad-contract.json)
- [onboarding-contract.json](../../parity-fixtures/translation-gummy/onboarding-contract.json)
- [volume-control.json](../../parity-fixtures/translation-gummy/volume-control.json)
- [state-contract.json](../../parity-fixtures/translation-gummy/state-contract.json)

## Current Implementation Debt

Windows, Android, and `vad-contract.json` currently emit/assert `thinkingBudget = 0` for every selectable Live model. That is a legacy 2.5 contract and is invalid for the selectable 3.1 model. Fix both setup builders, fixture shape, and tests together; do not paper over the mismatch in prose.
