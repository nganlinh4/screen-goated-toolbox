use super::RealtimeState;
use serde::Deserialize;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn finality_identity_and_retry_match_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/translation-request-identity.json"
    )))
    .unwrap();
    for case in fixture["finalityCases"].as_array().unwrap() {
        let mut state = RealtimeState::new();
        state.set_transcription_method(super::TranscriptionMethod::GeminiTranscribe);
        state.set_transcript_segments(
            case["initialCommitted"].as_str().unwrap(),
            case["initialDraft"].as_str().unwrap(),
        );
        let request = state.get_translation_request().unwrap();
        state.set_transcript_segments(
            case["currentCommitted"].as_str().unwrap(),
            case["currentDraft"].as_str().unwrap(),
        );
        assert_eq!(
            state.apply_translation_result(&request, "final", "draft"),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );

        let mut retry = RealtimeState::new();
        retry.set_transcription_method(super::TranscriptionMethod::GeminiTranscribe);
        retry.set_transcript_segments(
            case["initialCommitted"].as_str().unwrap(),
            case["initialDraft"].as_str().unwrap(),
        );
        let first = retry.get_translation_request().unwrap();
        assert!(retry.apply_translation_result(&first, "final", "draft"));
        retry.set_transcript_segments(
            case["currentCommitted"].as_str().unwrap(),
            case["currentDraft"].as_str().unwrap(),
        );
        assert_eq!(
            !retry.is_transcript_unchanged(),
            case["retry"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn translation_revision_changes_on_freeze_and_committed_source_reset() {
    let mut state = RealtimeState::new();
    state.committed_translation = "First".into();
    let initial = state.translation_revision;
    state.freeze_current_transcript();
    assert_ne!(state.translation_revision, initial);

    state.transcription_method = super::TranscriptionMethod::GeminiTranscribe;
    state.set_transcript_segments("Original source.", "");
    state.last_committed_pos = "Original source.".len();
    state.committed_translation = "First".into();
    let previous = state.translation_revision;
    state.set_transcript_segments("Replacement.", "");
    assert_ne!(state.translation_revision, previous);
    assert!(state.committed_translation.is_empty());
}

#[test]
fn shared_fixtures_match_windows_realtime_state() {
    let fixtures: FixtureDocument = serde_json::from_str(
        &fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/live-translate/state-machine.json"
        ))
        .expect("fixture file"),
    )
    .expect("fixture json");

    assert_eq!(fixtures.version, 4);
    assert_eq!(
        fixtures.text_llm_providers,
        ["google", "gemini-live", "groq", "nvidia"]
    );
    assert_eq!(
        fixtures.text_llm_request_policy.non_streaming_deadline,
        "benchmark_derived_10_to_30_seconds"
    );
    assert!(fixtures.text_llm_request_policy.circuit_preflight);
    assert!(fixtures.text_llm_request_policy.provider_blocking);
    assert_eq!(fixtures.text_llm_request_policy.cancellation, "propagate");
    assert!(fixtures.text_llm_request_policy.usage_headers);

    for case in fixtures.cases {
        let mut state = RealtimeState::new();
        for step in case.steps {
            match step.step_type.as_str() {
                "appendTranscript" => {
                    state.append_transcript(step.text.as_deref().unwrap_or_default());
                }
                "setTranscriptSegments" => {
                    state.set_transcript_segments(
                        step.committed.as_deref().unwrap_or_default(),
                        step.draft.as_deref().unwrap_or_default(),
                    );
                }
                "claimTranslationRequest" => {
                    let actual = state.get_translation_request();
                    match step.expected_request {
                        Some(expected) => {
                            let actual = actual.expect("translation request");
                            assert_eq!(
                                actual.source_start, expected.source_start,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.source_end, expected.source_end,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.finalized_source_end, expected.finalized_source_end,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.pending_source, expected.pending_source,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.finalized_source, expected.finalized_source,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.draft_source, expected.draft_source,
                                "case {}",
                                case.name
                            );
                            assert_eq!(
                                actual.previous_draft_translation,
                                expected.previous_draft_translation,
                                "case {}",
                                case.name
                            );
                        }
                        None => {
                            assert!(actual.is_none(), "case {}", case.name);
                        }
                    }
                }
                "applyTranslationResponse" => {
                    let request = state
                        .get_translation_request()
                        .expect("translation request before apply");
                    let response = step.response.expect("translation response");
                    let finalized_translation = response
                        .patches
                        .iter()
                        .find(|patch| patch.state == "final")
                        .map(|patch| patch.translation.as_str())
                        .unwrap_or("");
                    let draft_translation = response
                        .patches
                        .iter()
                        .find(|patch| patch.state == "draft")
                        .map(|patch| patch.translation.as_str())
                        .unwrap_or("");
                    assert!(
                        state.apply_translation_result(
                            &request,
                            finalized_translation,
                            draft_translation
                        ),
                        "case {}",
                        case.name
                    );
                }
                "forceCommitAll" => {
                    state.force_commit_all();
                }
                "freezeCurrentTranscript" => {
                    state.freeze_current_transcript();
                }
                "clearTranslationHistory" => {
                    state.translation_history.clear();
                }
                other => panic!("unknown fixture step type: {}", other),
            }
        }

        assert_eq!(
            state.frozen_prefix, case.expected_state.frozen_prefix,
            "case {}",
            case.name
        );
        assert_eq!(
            state.full_transcript, case.expected_state.full_transcript,
            "case {}",
            case.name
        );
        assert_eq!(
            state.display_transcript, case.expected_state.display_transcript,
            "case {}",
            case.name
        );
        assert_eq!(
            state.last_committed_pos, case.expected_state.last_committed_pos,
            "case {}",
            case.name
        );
        assert_eq!(
            state.last_processed_len, case.expected_state.last_processed_len,
            "case {}",
            case.name
        );
        assert_eq!(
            state.committed_translation, case.expected_state.committed_translation,
            "case {}",
            case.name
        );
        assert_eq!(
            state.uncommitted_translation, case.expected_state.uncommitted_translation,
            "case {}",
            case.name
        );
        assert_eq!(
            state.uncommitted_source_start, case.expected_state.uncommitted_source_start,
            "case {}",
            case.name
        );
        assert_eq!(
            state.uncommitted_source_end, case.expected_state.uncommitted_source_end,
            "case {}",
            case.name
        );
        assert_eq!(
            state.display_translation, case.expected_state.display_translation,
            "case {}",
            case.name
        );
        let actual_history: Vec<FixtureHistoryEntry> = state
            .translation_history
            .iter()
            .map(|(source, translation)| FixtureHistoryEntry {
                source: source.clone(),
                translation: translation.clone(),
            })
            .collect();
        assert_eq!(
            actual_history, case.expected_state.translation_history,
            "case {}",
            case.name
        );
    }
}

#[test]
fn gemini_force_commit_timeout_matches_windows_thresholds() {
    let mut state = RealtimeState::new();
    state.append_transcript("hello world");
    let request = state.get_translation_request().expect("request");
    assert!(state.apply_translation_result(&request, "", "hola mundo"));

    state.last_transcript_append_time = Instant::now() - Duration::from_millis(700);
    state.last_translation_update_time = Instant::now() - Duration::from_millis(900);
    assert!(!state.should_force_commit_on_timeout());

    state.last_transcript_append_time = Instant::now() - Duration::from_millis(900);
    state.last_translation_update_time = Instant::now() - Duration::from_millis(1001);
    assert!(state.should_force_commit_on_timeout());
}

#[test]
fn transcript_correction_rolls_translation_back_to_the_multibyte_divergence() {
    let mut state = RealtimeState::new();
    state.set_transcription_method(super::TranscriptionMethod::GeminiLive);
    state.set_transcript_segments("héllo wörld. next", "");
    state.last_committed_pos = state.full_transcript.len();
    state.set_transcript_segments("héllo wörld, next", "");
    assert_eq!(state.last_committed_pos, "héllo wörld".len());
}

#[test]
fn long_session_transcript_updates_stay_linear() {
    // A recognizer publishes the whole session transcript several times a
    // second while the shared state lock is held; each update must not scan
    // the transcript once per character.
    let committed = "word ".repeat(12_000);
    let mut state = RealtimeState::new();
    state.set_transcription_method(super::TranscriptionMethod::GeminiLive);
    state.set_transcript_segments(&committed, "draft");
    let started = Instant::now();
    for step in 0..10 {
        state.set_transcript_segments(&committed, &format!("draft {step}"));
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "ten updates of a 60k-char transcript took {:?}",
        started.elapsed()
    );
    assert!(state.full_transcript.ends_with("draft 9"));
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureDocument {
    version: u32,
    text_llm_providers: Vec<String>,
    text_llm_request_policy: FixtureTextLlmRequestPolicy,
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureTextLlmRequestPolicy {
    non_streaming_deadline: String,
    circuit_preflight: bool,
    provider_blocking: bool,
    cancellation: String,
    usage_headers: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    name: String,
    steps: Vec<FixtureStep>,
    expected_state: FixtureExpectedState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureStep {
    #[serde(rename = "type")]
    step_type: String,
    text: Option<String>,
    committed: Option<String>,
    draft: Option<String>,
    expected_request: Option<FixtureExpectedRequest>,
    response: Option<FixtureResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureExpectedRequest {
    source_start: usize,
    source_end: usize,
    finalized_source_end: usize,
    pending_source: String,
    finalized_source: String,
    draft_source: String,
    previous_draft_translation: String,
}

#[derive(Debug, Deserialize)]
struct FixtureResponse {
    patches: Vec<FixturePatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixturePatch {
    state: String,
    translation: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FixtureExpectedState {
    #[serde(default)]
    frozen_prefix: String,
    full_transcript: String,
    display_transcript: String,
    last_committed_pos: usize,
    last_processed_len: usize,
    committed_translation: String,
    uncommitted_translation: String,
    uncommitted_source_start: usize,
    uncommitted_source_end: usize,
    display_translation: String,
    translation_history: Vec<FixtureHistoryEntry>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct FixtureHistoryEntry {
    source: String,
    translation: String,
}
