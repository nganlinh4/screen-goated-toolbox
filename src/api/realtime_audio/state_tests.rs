use super::RealtimeState;
use serde::Deserialize;
use std::fs;
use std::time::{Duration, Instant};

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

    assert_eq!(fixtures.version, 2);

    for case in fixtures.cases {
        let mut state = RealtimeState::new();
        for step in case.steps {
            match step.step_type.as_str() {
                "appendTranscript" => {
                    state.append_transcript(step.text.as_deref().unwrap_or_default());
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
                "clearTranslationHistory" => {
                    state.translation_history.clear();
                }
                other => panic!("unknown fixture step type: {}", other),
            }
        }

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureDocument {
    version: u32,
    cases: Vec<FixtureCase>,
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
