---
name: gemini-live-api-dev
description: "Build, debug, or review Gemini Live API sessions in this repository: bidirectional audio/video, VAD, transcription, tools, interruption, session lifecycle, and model migration. Use for any Gemini WebSocket setup or realtime-input change."
---

# Gemini Live API Development

## Start Here

1. Read the feature's setup builder, receive loop, lifecycle owner, and tests.
2. Resolve the selected API model through `catalog/model_catalog.json`; do not assume one global Live model.
3. Check Google's current [Live overview](https://ai.google.dev/gemini-api/docs/live-api), [capabilities matrix](https://ai.google.dev/gemini-api/docs/live-api/capabilities), and [deprecations](https://ai.google.dev/gemini-api/docs/deprecations).
4. Build setup fields by model capability, not by a shared legacy payload.

## Model-Specific Invariants

- Gemini 3.1 Flash Live uses `thinkingLevel`; Gemini 2.5 Flash Live uses `thinkingBudget`.
- Gemini 3.1 may place audio and transcripts in the same server event. Process every part.
- For Gemini 3.1, send conversational text with realtime input; client content is only for seeded history with the matching history configuration.
- Do not enable proactive audio, affective dialog, or non-blocking function calling on a model that does not support it.
- Input audio is raw little-endian PCM16; declare its sample rate. Output is PCM16 at the model's documented rate.
- On interruption, stop playback and discard queued response audio for the interrupted turn.
- Handle setup completion, errors, turn completion, `goAway`, reconnect, cancellation, and stale-session isolation explicitly.

## Repository Map

- Shared model catalog: `catalog/model_catalog.json`
- General Live transport: `src/api/gemini_live/`
- Realtime audio: `src/api/realtime_audio/`
- Translation Gummy: `src/overlay/translation_gummy/`
- Computer Control: `src/overlay/computer_control/`
- Android Live clients: `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/`
- Shared parity contracts: `.claude/parity/` and `parity-fixtures/`

## Verification

- Assert the complete setup JSON for every supported model family.
- Test multi-part server events, interruption, turn completion, malformed events, reconnect, and cancellation.
- Verify both sides of Windows/Android parity when a shared feature changes.
- Never place credentials or raw user audio in fixtures or logs.
