use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::overlay::computer_control::protocol::ServerEvent;
use crate::overlay::computer_control::runtime::reader::handle_event;

fn transcript(state: &mut Reader, text: &str) {
    let (sender, _receiver) = mpsc::channel();
    handle_event(
        ServerEvent::InputTranscript(text.into()),
        None,
        &sender,
        state,
    );
}

fn event(state: &mut Reader, event: ServerEvent) {
    let (sender, _receiver) = mpsc::channel();
    handle_event(event, None, &sender, state);
}

fn users(state: &Reader) -> Vec<&str> {
    state
        .history
        .iter()
        .filter_map(|line| line.strip_prefix("User: "))
        .collect()
}

#[test]
fn pause_inside_collecting_input_keeps_one_epoch_goal_and_history_record() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    let mut tracker = UserAudioTracker::default();
    let t0 = Instant::now() - Duration::from_secs(2);
    assert!(tracker.update_at(true, 0.3, 960, t0));
    begin_epoch(&mut state);
    transcript(&mut state, "perform the first operation");
    tracker.commit_transcript("test");
    let epoch = state.input_transcript.epoch();
    let started = state.turn_started_at;
    assert!(!tracker.update_at(false, 0.0, 960, t0 + Duration::from_millis(750)));
    // Resume after the detector's real silence boundary, without a response.
    observe(&mut tracker, &mut state, true, 0.3, 960, false);
    assert!(!state.input_transcript.has_fresh_epoch());
    transcript(&mut state, "with the original order");
    assert_eq!(state.input_transcript.epoch(), epoch);
    assert_eq!(state.turn_started_at, started);
    assert_eq!(
        users(&state),
        ["perform the first operation with the original order"]
    );
    assert_eq!(
        state.last_cmd,
        "perform the first operation with the original order"
    );
}

#[test]
fn speech_onset_after_completion_opens_input_during_the_playback_tail() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "first request");
    event(
        &mut state,
        ServerEvent::OutputTranscript("first answer".into()),
    );
    event(&mut state, ServerEvent::TurnComplete);
    assert!(state.terminal_drain && state.terminal_boundary_seen);
    let mut tracker = UserAudioTracker::default();
    observe(&mut tracker, &mut state, true, 0.3, 960, true);
    assert!(state.input_transcript.has_fresh_epoch());
    assert!(state.frame_delivery.pending());
    assert!(state.source_frame.is_none());
    transcript(&mut state, "second request");
    assert_eq!(users(&state), ["first request", "second request"]);
    assert!(state.active && state.awaiting && !state.terminal_drain);
}

#[test]
fn voice_started_before_the_boundary_can_continue_into_completed_playback_tail() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "first request");
    event(
        &mut state,
        ServerEvent::OutputTranscript("first answer".into()),
    );
    let mut tracker = UserAudioTracker::default();
    observe(&mut tracker, &mut state, true, 0.3, 960, true);
    assert!(!state.input_transcript.has_fresh_epoch());
    event(&mut state, ServerEvent::TurnComplete);
    observe(&mut tracker, &mut state, true, 0.3, 960, true);
    assert!(state.input_transcript.has_fresh_epoch());
    transcript(&mut state, "next request");
    assert_eq!(users(&state), ["first request", "next request"]);
}

#[test]
fn completed_boundary_without_new_voice_keeps_late_asr_as_a_revision() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "perform the fir");
    event(&mut state, ServerEvent::TurnComplete);
    let mut tracker = UserAudioTracker::default();
    for playing in [true, false, true] {
        observe(&mut tracker, &mut state, false, 0.0, 960, playing);
    }
    assert!(!state.input_transcript.has_fresh_epoch());
    transcript(&mut state, "perform the first operation");
    assert_eq!(users(&state), ["perform the first operation"]);
    assert!(state.terminal_drain && !state.active && !state.awaiting);
}

#[test]
fn continuous_speech_does_not_start_another_epoch_when_response_progress_arrives() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    let mut tracker = UserAudioTracker::default();
    observe(&mut tracker, &mut state, true, 0.3, 960, false);
    transcript(&mut state, "first part");
    event(&mut state, ServerEvent::Thought("response progress".into()));
    observe(&mut tracker, &mut state, true, 0.3, 960, false);
    assert!(!state.input_transcript.has_fresh_epoch());
    // Starting a new acoustic segment after response progress is still not a
    // provider interruption or a completed response boundary.
    let mut resumed = UserAudioTracker::default();
    observe(&mut resumed, &mut state, true, 0.3, 960, false);
    assert!(!state.input_transcript.has_fresh_epoch());
    transcript(&mut state, "second part");
    assert_eq!(users(&state), ["first part second part"]);
}

#[test]
fn completed_speech_candidate_does_not_create_a_turn_without_a_transcript() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "request");
    event(&mut state, ServerEvent::TurnComplete);
    let mut tracker = UserAudioTracker::default();
    observe(&mut tracker, &mut state, true, 0.02, 960, true);
    assert!(state.input_transcript.has_fresh_epoch());
    assert_eq!(users(&state), ["request"]);
    assert!(state.terminal_drain && !state.active && !state.awaiting);
}

#[test]
fn typed_interruption_during_terminal_drain_also_retires_the_visual_source() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "request");
    event(&mut state, ServerEvent::TurnComplete);
    event(&mut state, ServerEvent::Interrupted);
    assert!(state.input_transcript.has_fresh_epoch());
    assert!(state.frame_delivery.pending());
    assert!(state.source_frame.is_none());
}

#[test]
fn completing_a_later_input_epoch_rearms_continuing_voice_once() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    let mut tracker = UserAudioTracker::default();
    transcript(&mut state, "first");
    event(&mut state, ServerEvent::TurnComplete);
    observe(&mut tracker, &mut state, true, 0.3, 960, true);
    transcript(&mut state, "second");
    event(&mut state, ServerEvent::TurnComplete);
    observe(&mut tracker, &mut state, true, 0.3, 960, true);
    transcript(&mut state, "third");
    assert_eq!(users(&state), ["first", "second", "third"]);
}

#[test]
fn provider_interruption_allows_confirmed_speech_to_supersede_a_silent_action() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "first request");
    let cancel = Arc::new(AtomicBool::new(false));
    state.pending = super::super::reader::Pending {
        id: Some("owned-job".into()),
        tool: Some("future_operation".into()),
        cancel: Some(Arc::clone(&cancel)),
        ..Default::default()
    };
    let mut tracker = UserAudioTracker::default();
    observe(&mut tracker, &mut state, true, 0.3, 960, false);
    assert!(!state.input_transcript.has_fresh_epoch());
    assert!(!state.pending.cancelled && !cancel.load(Ordering::SeqCst));
    event(&mut state, ServerEvent::Interrupted);
    assert!(state.input_transcript.has_fresh_epoch());
    assert!(!state.pending.cancelled && !cancel.load(Ordering::SeqCst));
    transcript(&mut state, "updated request");
    assert!(state.pending.cancelled && cancel.load(Ordering::SeqCst));
    assert_eq!(users(&state), ["first request", "updated request"]);
}

#[test]
fn cancellation_arms_input_only_for_the_owned_action() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    transcript(&mut state, "first request");
    let cancel = Arc::new(AtomicBool::new(false));
    state.pending = super::super::reader::Pending {
        id: Some("owned-job".into()),
        tool: Some("future_operation".into()),
        cancel: Some(Arc::clone(&cancel)),
        ..Default::default()
    };
    let playback_epoch = state.playback_epoch;
    event(
        &mut state,
        ServerEvent::ToolCancellation(vec!["retired-job".into()]),
    );
    assert_eq!(state.playback_epoch, playback_epoch);
    assert!(!state.input_transcript.has_fresh_epoch());
    assert!(!cancel.load(Ordering::SeqCst));
    event(
        &mut state,
        ServerEvent::ToolCancellation(vec!["owned-job".into()]),
    );
    assert!(state.input_transcript.has_fresh_epoch());
    assert!(state.frame_delivery.pending());
    assert!(cancel.load(Ordering::SeqCst));
    transcript(&mut state, "new request");
    assert_eq!(users(&state), ["first request", "new request"]);
}

#[test]
fn shared_speech_input_epoch_replays_match_android() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/phone-control/speech-input-epochs.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let _owner = super::super::super::session_retention::start_session();
        let mut state = Reader::default();
        let mut tracker = UserAudioTracker::default();
        for step in case["steps"].as_array().unwrap() {
            match step["event"].as_str().unwrap() {
                "input" => transcript(&mut state, step["text"].as_str().unwrap()),
                "output" => event(
                    &mut state,
                    ServerEvent::OutputTranscript(step["text"].as_str().unwrap().into()),
                ),
                "complete" => event(&mut state, ServerEvent::TurnComplete),
                "interrupt" => event(&mut state, ServerEvent::Interrupted),
                activity @ ("speech_start" | "speech_continues") => {
                    if activity == "speech_start" {
                        tracker = UserAudioTracker::default();
                    }
                    observe(
                        &mut tracker,
                        &mut state,
                        true,
                        0.3,
                        960,
                        step["playing"].as_bool().unwrap(),
                    );
                }
                unknown => panic!("unknown speech replay event: {unknown}"),
            }
        }
        let expected = case["userTurns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| text.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(users(&state), expected, "{}", case["name"]);
        assert_eq!(
            state.active,
            case["active"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn connection_replacement_keeps_collecting_input_and_rearms_idle_input() {
    let _owner = super::super::super::session_retention::start_session();
    for accepted in [false, true] {
        let mut state = Reader::default();
        transcript(&mut state, "first part");
        assert!(!super::super::terminal_drain::retire_for_connection_replacement(&mut state));
        let mut tracker = UserAudioTracker::default();
        observe(&mut tracker, &mut state, true, 0.3, 960, false);
        transcript(&mut state, "second part");
        assert_eq!(users(&state), ["first part second part"]);
        super::super::reader_policy::begin_terminal_drain(&mut state, accepted, true);
        assert_eq!(
            super::super::terminal_drain::retire_for_connection_replacement(&mut state),
            !accepted
        );
        let mut tracker = UserAudioTracker::default();
        observe(&mut tracker, &mut state, true, 0.3, 960, true);
        transcript(&mut state, "new request");
        assert_eq!(users(&state), ["first part second part", "new request"]);
    }
}
