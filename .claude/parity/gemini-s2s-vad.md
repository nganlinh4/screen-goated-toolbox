# Gemini S2S (Gemini Translate) Client-VAD Parity

## Canonical Source
- Windows (canonical): [src/api/realtime_audio/s2s.rs](../../src/api/realtime_audio/s2s.rs) (constants + `is_segment_worth_sending`), [src/api/realtime_audio/s2s/utils.rs](../../src/api/realtime_audio/s2s/utils.rs) (accept rule + ratios)
- Android: [mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sVad.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sVad.kt)

## Behavior Contract
The client-side voice-activity / segmentation / send-gating logic for Gemini S2S
(Gemini Translate live audio) is hand-ported between Windows and Android. Windows
is canonical. The tuning constants and the accept rule must stay in lock-step.

**Constants (locked by fixture).** The ~24 VAD/segmentation/timeout constants
(`FRAME_SAMPLES`, `PREROLL_SAMPLES`, `MIN/TARGET/MAX_SEGMENT_SAMPLES`,
`END_SILENCE_FRAMES`, the speech-threshold + noise-learn floats, the
`MIN_SPEECH_LIKE_RATIO`/`STRICT_*` gates, the first-audio retry + hedge timeouts,
`SESSION_COUNT`) are asserted equal on both platforms via the shared fixture.

**Accept rule.** `is_segment_worth_sending` / `isSegmentWorthSending` gates whether
a captured segment is worth sending. The strict-path confidence branch requires a
`speech_like_ratio >= 0.08` floor *in addition to* the confidence threshold —
because a high blended confidence can come purely from energy terms (loud
flat/tonal/DC noise). Android had dropped this floor (fixed 2026-06; it accepted
noise Windows rejects). The rule itself is currently locked only by independent
per-platform unit tests (`s2s_adaptive_vad_*` in s2s.rs; the Android VAD tests),
NOT yet by a shared rule fixture — see Follow-up.

## Fixtures
- Constants fixture: [parity-fixtures/gemini-s2s-vad/constants.json](../../parity-fixtures/gemini-s2s-vad/constants.json).
- Rust assertion: `s2s_vad_constants_match_parity_fixture` in [src/api/realtime_audio/s2s.rs](../../src/api/realtime_audio/s2s.rs).
- Android assertion: [mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sVadConstantsParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sVadConstantsParityTest.kt).

## Follow-up (not yet locked)
- A shared *rule* fixture: cases of `(speechFrames, peakRms, meanRms,
  speechLikeFrames, energeticFrames, sampleCount, strictness) -> expected accept`
  derived from the Windows rule, asserted by both a Rust and an Android unit test,
  to lock the accept logic (and the ratio/confidence math) — not just its literals.
- The Gemini S2S setup-payload field contract + the BCP-47 target-language table
  (`transport.rs` + `websocket.rs::live_translate_target_language_code` vs
  `GeminiS2sProtocol.kt`) are similarly hand-duplicated with no shared fixture.
- The grouped-timeout formulas (`grouped_first_audio_timeout_ms` clamp 5500/30000,
  `grouped_hard_timeout_ms` min 180000, ×2/×4 multipliers) are duplicated and not
  yet fixture-locked.

## Changing this behavior
Edit the Windows canonical, mirror onto Android, and update `constants.json` so
both suites assert the new values. Behavioral VAD changes should also be validated
on-device before relying on them.
