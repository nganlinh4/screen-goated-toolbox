# Offline-ASR Streaming Commit Parity

## Canonical Source
- Windows commit machine (canonical): [src/api/realtime_audio/offline_asr_commit.rs](../../src/api/realtime_audio/offline_asr_commit.rs)
- Windows streaming loop that drives it: [src/api/realtime_audio/sherpa_onnx/streaming.rs](../../src/api/realtime_audio/sherpa_onnx/streaming.rs) (`run_streaming_loop` → `handle_recognizer_result` → `offline_asr_commit_step`)
- Android port (commonMain): [mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/live/OfflineAsrStreamParity.kt](../../mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/live/OfflineAsrStreamParity.kt)
- Android runtime that drives it: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntimeOfflineAsr.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntimeOfflineAsr.kt) (`runSherpaSession`)

## Behavior Contract
Both platforms run sherpa-onnx `OnlineRecognizer` (Zipformer) in a poll loop: feed
audio in 500 ms batches, decode while ready, read the recognizer's **full** decoded
text each tick, and feed that text to a single pure commit machine. The machine owns
all segmentation; the recognizer is never reset and its endpoint detector is never
used (`enable_endpoint = 0` / `enableEndpoint = false`).

The machine carries this state (starting empty), mutated in place per tick:
- `committedHistory` — finalized text, segments joined with smart spacing.
- `streamCommittedPrefix` — the recognizer-text prefix already committed; stripped off
  each new full result so only the uncommitted tail is shown as the draft.
- `lastDraftText` / `lastDraftChangeMs` — the current draft and the monotonic-ms time it
  last changed; drives silence timing.

Per tick, given the trimmed recognizer text, `hasNativePunctuation`, and a monotonic
`nowMs`, `commitStep` does:
1. **Draft** = recognizer text with `streamCommittedPrefix` stripped (trim-start). If the
   text no longer starts with the prefix, the prefix is treated as empty and the whole
   text becomes the draft.
2. If the draft changed since last tick, reset `lastDraftChangeMs = nowMs`.
3. **Native-punctuation models** (EN/KO/FR/DE/ES):
   - **Interior sentence boundary** — if the draft has a `.?!` followed by more
     alphanumeric text, commit everything up to and including the boundary; the
     remainder becomes the new draft (prefix updated to the committed span).
   - **Punctuation-terminated + stale** — if the draft ends in `.?!` and has been stable
     for ≥ `PUNCT_STALE_COMMIT_MS` (600 ms), commit the whole draft; draft → empty.
   - Otherwise show the draft as-is.
4. **Non-native-punctuation models** (ZH/RU/All8): commit when the draft has been stable
   past a word-count-scaled threshold `1200 / (1 + words·0.5)` ms (each CJK char counts as
   one word); commit appends a period. Otherwise show the draft as-is.
5. Returns the active (uncommitted) draft. The caller publishes
   `setTranscriptSegments(committedHistory, activeDraft)`.

`committedHistory` / `streamCommittedPrefix` updates and the returned draft must match the
golden fixtures exactly on both platforms.

## Platform Entry Points
- Windows: real-time transcription overlay with a local Zipformer model selected
  (`SherpaZipformer` transcription method).
- Android: Live session with a Zipformer model — `runSherpaSession` poll loop.

## Deliberate Deviations
- **Android Moonshine path is event-driven, not this machine.** `runMoonshineSession`
  uses the Moonshine library's own `onLineTextChanged` / `onLineCompleted` line events for
  draft/commit. That is a library-native segmentation model with no Windows equivalent and
  is intentionally **not** routed through `commitStep`. Only the polled sherpa/Zipformer
  path is covered by this parity contract.
- **Unicode units.** The Windows reference slices/measures in UTF-8 bytes; the Kotlin port
  measures in UTF-16 chars. The two agree for ASCII and BMP text (including the common CJK
  range). The single untested edge is a sentence boundary immediately followed by
  supplementary-plane (astral) text; no current model emits that, so it is documented here
  rather than special-cased.

## Removed Divergences (pre-parity Android glue)
The Android port previously carried logic with **no Windows equivalent**, since removed:
- a `recognizer.isEndpoint()` branch that committed and called `recognizer.reset(stream)`,
- `enableEndpoint = true`,
- a `DRAFT_STALE_MS = 3_000` ms hard cap that force-committed / appended a period to the
  draft after 3 s regardless of the model's own threshold.

## Fixtures
- Golden fixtures: [parity-fixtures/offline-asr-stream/cases.json](../../parity-fixtures/offline-asr-stream/cases.json)
  - Each case starts from empty state and runs `steps` in order; after each step both
    `expectCommittedHistory` and the returned `expectActiveDraft` must match.
  - Step fields (camelCase): `text`, `hasNativePunctuation`, `nowMs`.
- Rust assertion: `shared_fixtures_match_offline_asr_commit` in [src/api/realtime_audio/state_tests.rs](../../src/api/realtime_audio/state_tests.rs).
- Android assertion: [mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/OfflineAsrStreamParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/OfflineAsrStreamParityTest.kt).

## Changing this behavior
Edit `offline_asr_commit.rs` (canonical) and mirror into `OfflineAsrStreamParity.kt`, then
update `cases.json` so both suites assert the new contract. Neither platform may change its
commit behavior without updating the fixture and this spec.
