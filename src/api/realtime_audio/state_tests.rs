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

    assert_eq!(fixtures.version, 1);

    for case in fixtures.cases {
        let mut state = RealtimeState::new();
        for step in case.steps {
            match step.step_type.as_str() {
                "appendTranscript" => {
                    state.append_transcript(step.text.as_deref().unwrap_or_default());
                }
                "claimTranslationRequest" => {
                    let actual = state.get_translation_chunk();
                    match step.expected_request {
                        Some(expected) => {
                            let actual = actual.expect("translation request");
                            assert_eq!(actual.0, expected.chunk, "case {}", case.name);
                            assert_eq!(
                                actual.1, expected.has_finished_delimiter,
                                "case {}",
                                case.name
                            );
                            assert_eq!(actual.2, expected.bytes_to_commit, "case {}", case.name);
                            state.update_last_processed_len();
                            state.start_new_translation();
                        }
                        None => {
                            assert!(actual.is_none(), "case {}", case.name);
                        }
                    }
                }
                "appendTranslation" => {
                    state.append_translation(step.text.as_deref().unwrap_or_default());
                }
                "finalizeTranslation" => {
                    state.commit_current_translation();
                    state.advance_committed_pos(step.bytes_to_commit.unwrap_or_default());
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
    state.update_last_processed_len();
    state.start_new_translation();
    state.append_translation("hola mundo");

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
    bytes_to_commit: Option<usize>,
    expected_request: Option<FixtureExpectedRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureExpectedRequest {
    chunk: String,
    has_finished_delimiter: bool,
    bytes_to_commit: usize,
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
    display_translation: String,
    translation_history: Vec<FixtureHistoryEntry>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct FixtureHistoryEntry {
    source: String,
    translation: String,
}
