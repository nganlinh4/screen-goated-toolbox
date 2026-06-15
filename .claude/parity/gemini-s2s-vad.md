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
noise Windows rejects). The rule is now locked by the shared `accept-rule.json`
fixture below (one case directly exercises the 0.08 floor).

## Fixtures
- Constants fixture: [parity-fixtures/gemini-s2s-vad/constants.json](../../parity-fixtures/gemini-s2s-vad/constants.json).
- Rust assertion: `s2s_vad_constants_match_parity_fixture` in [src/api/realtime_audio/s2s.rs](../../src/api/realtime_audio/s2s.rs).
- Android assertion: [mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sVadConstantsParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sVadConstantsParityTest.kt).

**Accept rule + grouped-timeout formulas** (locked by fixture):
- Rule cases: [parity-fixtures/gemini-s2s-vad/accept-rule.json](../../parity-fixtures/gemini-s2s-vad/accept-rule.json) — `(frameCount, speechFrames, speechLikeFrames, energeticFrames, peakRms, meanRms, strictness) -> expectAccept`, including a case that directly exercises the 0.08 speech-like floor. Asserts the baseline gate, strict/lenient paths, AND the ratio/confidence math.
- Timeout cases: [parity-fixtures/gemini-s2s-vad/timeouts.json](../../parity-fixtures/gemini-s2s-vad/timeouts.json) — `grouped_first_audio_timeout_ms` (clamp 5500/30000, ×2) and `grouped_hard_timeout_ms` (min 180000, ×4).
- Rust: `s2s_accept_rule_matches_parity_fixture` + `s2s_grouped_timeouts_match_parity_fixture` in s2s.rs. Android: [GeminiS2sVadRuleParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sVadRuleParityTest.kt).

**Target-language BCP-47 mapping** (explicit special cases locked by fixture):
[parity-fixtures/gemini-s2s-language/target-language-codes.json](../../parity-fixtures/gemini-s2s-language/target-language-codes.json) — `input -> expect` for the Chinese/Portuguese/Filipino/Norwegian special cases + empty. Rust: `target_language_codes_match_parity_fixture` in websocket.rs. Android: [GeminiS2sTargetLanguageParityTest.kt](../../mobile/androidApp/src/test/java/dev/screengoated/toolbox/mobile/parity/GeminiS2sTargetLanguageParityTest.kt). The general name->code fallback (Windows `isolang` vs Android `LanguageCatalog`) is a documented deviation and not locked.

## Follow-up (not yet locked)
- The Gemini S2S setup-payload field contract (`transport.rs build_s2s_setup_payload`
  + `websocket.rs build_live_translate_setup_value` vs `GeminiS2sProtocol.kt
  buildGeminiS2sSetupPayload`) — the nested JSON wire format — is hand-duplicated
  with no shared fixture. (Lower drift risk: a field change breaks the live API on
  one platform immediately; still worth a structural fixture eventually.)

## Changing this behavior
Edit the Windows canonical, mirror onto Android, and update the relevant fixture so
both suites assert the new values. Behavioral VAD changes should also be validated
on-device before relying on them.
