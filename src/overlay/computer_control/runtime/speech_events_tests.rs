use super::{
    ASR_TRANSCRIPT_DEADLINE, InputTranscriptAssembler, LOW_CONFIDENCE_RETIRE_DEADLINE,
    MIN_TIMEOUT_ACTIVITY, MIN_TIMEOUT_PEAK_LEVEL, PlaybackTracker, Reader, UserAudioRecovery,
    UserAudioTracker, audio, discard_generation_audio, transcript,
};
use std::time::Instant;

#[test]
fn uploaded_voice_remains_uncommitted_until_a_transcript_arrives() {
    let mut tracker = UserAudioTracker::default();
    assert!(tracker.update(true, 0.5, 960));
    assert!(!tracker.update(true, 0.5, 960));
    assert!(tracker.has_uncommitted_audio());
    tracker.commit_transcript("test");
    assert!(!tracker.has_uncommitted_audio());
}

#[test]
fn text_only_turn_cannot_inherit_prior_audio_duration() {
    let mut tracker = UserAudioTracker {
        last_evidence: Some(super::UserAudioEvidence {
            duration_ms: 900,
            ..super::UserAudioEvidence::default()
        }),
        ..UserAudioTracker::default()
    };
    tracker.commit_transcript("local_text");
    assert!(tracker.last_evidence.is_none());
}

#[test]
fn playback_echo_onset_does_not_open_a_fresh_input_epoch() {
    let mut tracker = UserAudioTracker::default();
    let mut transcript = InputTranscriptAssembler::default();
    assert!(transcript.merge("committed request").unwrap().starts_turn);
    if tracker.update_for_local_epoch(true, 0.5, 960, true) {
        transcript.begin_epoch();
    }
    assert!(!transcript.merge("speaker echo").unwrap().starts_turn);
}

#[test]
fn non_playback_voiced_onset_opens_a_fresh_input_epoch() {
    let mut tracker = UserAudioTracker::default();
    let mut transcript = InputTranscriptAssembler::default();
    assert!(transcript.merge("committed request").unwrap().starts_turn);
    if tracker.update_for_local_epoch(true, 0.5, 960, false) {
        transcript.begin_epoch();
    }
    let next = transcript.merge("new request").unwrap();
    assert!(next.starts_turn);
    assert_eq!(next.text, "new request");
}

#[test]
fn typed_output_transcript_is_never_content_filtered() {
    let mut state = Reader::default();
    transcript(&mut state, "{\"spoken\":true}", None);
    assert_eq!(state.reply, "{\"spoken\":true}");
    assert_eq!(state.reasoning, state.reply);
    assert_eq!(state.reply_utterance_id, state.assistant_utterance_id);
}

#[test]
fn barge_in_keeps_caption_correlated_to_the_interrupted_utterance() {
    let mut state = Reader::default();
    transcript(&mut state, "partial reply", None);
    let owner = state.assistant_utterance_id;
    super::interrupted(&mut state, None);
    assert!(state.assistant_utterance_id.is_none());
    assert_eq!(state.reply_utterance_id, owner);
}

#[test]
fn typed_audio_is_processed_before_any_transcript() {
    let mut state = Reader::default();
    audio(&mut state, &[1, -1, 2, -2], None);
    assert!(state.assistant_utterance_id.is_some());
    assert_eq!(state.generation_audio.len(), 4);
    assert!(state.reply.is_empty());
}

#[test]
fn rejected_generation_discards_held_audio() {
    let mut state = Reader::default();
    audio(&mut state, &[1, -1, 2, -2], None);
    discard_generation_audio(&mut state, "test_rejection");
    assert_eq!(state.generation_audio.len(), 0);
}

#[test]
fn one_utterance_reports_only_one_start_across_a_short_underflow() {
    let mut tracker = PlaybackTracker::default();
    assert!(tracker.register_start(Some(7)));
    assert!(!tracker.register_start(Some(7)));
    assert!(tracker.register_start(Some(8)));
}

#[test]
fn interruption_epoch_retires_tracker_without_a_later_completion() {
    let mut tracker = PlaybackTracker::default();
    let mut state = Reader {
        assistant_utterance_id: Some(7),
        ..Reader::default()
    };
    tracker.update(true, &mut state, None);
    assert_eq!(tracker.started_utterance, Some(7));
    state.assistant_utterance_id = None;
    state.playback_epoch += 1;
    tracker.update(false, &mut state, None);
    assert_eq!(tracker.epoch, state.playback_epoch);
    assert_eq!(tracker.started_utterance, None);
    assert_eq!(tracker.quiet_since, None);
}

#[test]
fn ended_voice_without_transcript_expires_once() {
    let mut tracker = UserAudioTracker {
        uncommitted: true,
        ended_uncommitted_at: Some(Instant::now() - ASR_TRANSCRIPT_DEADLINE),
        last_evidence: Some(super::UserAudioEvidence {
            duration_ms: MIN_TIMEOUT_ACTIVITY.as_millis(),
            peak_level: MIN_TIMEOUT_PEAK_LEVEL,
            voiced_samples: 960,
        }),
        ..UserAudioTracker::default()
    };
    assert_eq!(
        tracker.report_missing_transcript(),
        Some(UserAudioRecovery::TranscriptTimedOut)
    );
    assert!(!tracker.has_uncommitted_audio());
    assert_eq!(tracker.report_missing_transcript(), None);
}

#[test]
fn short_quiet_activity_expires_without_claiming_an_asr_failure() {
    let mut tracker = UserAudioTracker {
        uncommitted: true,
        ended_uncommitted_at: Some(Instant::now() - LOW_CONFIDENCE_RETIRE_DEADLINE),
        last_evidence: Some(super::UserAudioEvidence {
            duration_ms: MIN_TIMEOUT_ACTIVITY.as_millis() - 1,
            peak_level: MIN_TIMEOUT_PEAK_LEVEL / 2.0,
            voiced_samples: 800,
        }),
        ..UserAudioTracker::default()
    };
    assert_eq!(
        tracker.report_missing_transcript(),
        Some(UserAudioRecovery::LowConfidenceActivityExpired)
    );
    assert!(!tracker.has_uncommitted_audio());
}

#[test]
fn input_transcript_merges_cumulative_overlap_and_duplicates() {
    let mut transcript = InputTranscriptAssembler::default();
    let first = transcript.merge("perform the fir").unwrap();
    assert!(first.starts_turn);
    assert_eq!(first.text, "perform the fir");
    let cumulative = transcript.merge("perform the first operation").unwrap();
    assert!(!cumulative.starts_turn);
    assert_eq!(cumulative.text, "perform the first operation");
    let overlap = transcript.merge("operation with confirmation").unwrap();
    assert_eq!(
        overlap.text,
        "perform the first operation with confirmation"
    );
    assert!(!transcript.merge("with confirmation").unwrap().changed);
}

#[test]
fn input_transcript_starts_new_turn_only_when_lifecycle_opens_an_epoch() {
    let mut transcript = InputTranscriptAssembler::default();
    assert!(transcript.merge("first").unwrap().starts_turn);
    assert!(!transcript.merge("second").unwrap().starts_turn);
    transcript.begin_epoch();
    assert!(transcript.merge("new turn").unwrap().starts_turn);
}
