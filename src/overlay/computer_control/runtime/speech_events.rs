//! Correlated assistant speech and playback telemetry.

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::telemetry::{self, Privacy};
use super::reader::Reader;
use std::time::{Duration, Instant};

const ASR_TRANSCRIPT_DEADLINE: Duration = Duration::from_secs(5);
const MIN_TRANSCRIPT_OVERLAP_CHARS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputTranscriptUpdate {
    pub(super) text: String,
    pub(super) starts_turn: bool,
    pub(super) changed: bool,
}

/// Joins Live input-transcription deltas within one user turn. The endpoint may
/// send a growing cumulative string, overlapping chunks, or exact duplicates.
/// Lifecycle code resets this only at a structural user/model turn boundary.
#[derive(Default)]
pub(super) struct InputTranscriptAssembler {
    text: String,
    open: bool,
}

impl InputTranscriptAssembler {
    pub(super) fn merge(&mut self, fragment: &str) -> Option<InputTranscriptUpdate> {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            return None;
        }
        let starts_turn = !self.open;
        if starts_turn {
            self.text.clear();
            self.text.push_str(fragment);
            self.open = true;
            return Some(InputTranscriptUpdate {
                text: self.text.clone(),
                starts_turn,
                changed: true,
            });
        }
        let before = self.text.clone();
        merge_transcript_text(&mut self.text, fragment);
        Some(InputTranscriptUpdate {
            text: self.text.clone(),
            starts_turn,
            changed: self.text != before,
        })
    }

    pub(super) fn reset(&mut self) {
        self.text.clear();
        self.open = false;
    }
}

fn merge_transcript_text(existing: &mut String, incoming: &str) {
    let current = existing.trim();
    if current.is_empty() || incoming.starts_with(current) {
        existing.clear();
        existing.push_str(incoming);
        return;
    }
    if current.starts_with(incoming) || current.ends_with(incoming) {
        return;
    }
    let overlap = incoming
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(incoming.len()))
        .filter(|&length| {
            length > 0 && length <= current.len() && current.ends_with(&incoming[..length])
        })
        .max()
        .unwrap_or(0);
    let meaningful = overlap > 0
        && (incoming[..overlap].chars().any(char::is_whitespace)
            || incoming[..overlap].chars().count() >= MIN_TRANSCRIPT_OVERLAP_CHARS);
    if meaningful {
        existing.push_str(&incoming[overlap..]);
    } else {
        if !existing.ends_with(char::is_whitespace) {
            existing.push(' ');
        }
        existing.push_str(incoming);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UserAudioRecovery {
    TranscriptTimedOut,
}

#[derive(Default)]
pub(super) struct PlaybackTracker {
    was_playing: bool,
    quiet_since: Option<Instant>,
    started_at_sample: Option<u64>,
}

impl PlaybackTracker {
    pub(super) fn update(&mut self, playing: bool, state: &mut Reader, sink: Option<&AudioSink>) {
        if playing && !self.was_playing {
            self.started_at_sample = sink.map(AudioSink::played_samples);
            telemetry::event(
                "assistant_playback_started",
                "speech",
                Privacy::Safe,
                serde_json::json!({"utterance_id": state.assistant_utterance_id}),
            );
        }
        if playing {
            self.quiet_since = None;
        } else if self.was_playing {
            self.quiet_since = Some(Instant::now());
        }
        self.was_playing = playing;
        if self
            .quiet_since
            .is_some_and(|started| started.elapsed() > Duration::from_millis(1200))
        {
            overlay::set_model_idle();
            telemetry::event(
                "assistant_playback_completed",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "utterance_id": state.assistant_utterance_id,
                    "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
                    "played_output_samples": sink.map(AudioSink::played_samples).unwrap_or(0)
                        .saturating_sub(self.started_at_sample.unwrap_or(0)),
                }),
            );
            state.assistant_utterance_id = None;
            self.started_at_sample = None;
            self.quiet_since = None;
        }
    }
}

#[derive(Default)]
pub(super) struct UserAudioTracker {
    active_since: Option<Instant>,
    last_active: Option<Instant>,
    uncommitted: bool,
    ended_uncommitted_at: Option<Instant>,
}

impl UserAudioTracker {
    pub(super) fn update(&mut self, voiced: bool, level: f32, samples: usize) {
        if voiced && self.active_since.is_none() {
            self.active_since = Some(Instant::now());
            self.last_active = self.active_since;
            self.uncommitted = true;
            self.ended_uncommitted_at = None;
            telemetry::event(
                "user_audio_activity_started",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "detector": "rms_threshold",
                    "level": level,
                    "samples": samples,
                }),
            );
        } else if voiced {
            self.last_active = Some(Instant::now());
        } else if self
            .last_active
            .is_some_and(|last| last.elapsed() > Duration::from_millis(500))
            && let Some(started) = self.active_since.take()
        {
            telemetry::event(
                "user_audio_activity_ended",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "detector": "rms_threshold",
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            self.last_active = None;
            if self.uncommitted {
                self.ended_uncommitted_at = Some(Instant::now());
            }
        }
    }

    pub(super) fn commit_transcript(&mut self) {
        self.uncommitted = false;
        self.ended_uncommitted_at = None;
    }

    pub(super) fn has_uncommitted_audio(&self) -> bool {
        self.uncommitted
    }

    fn expire_missing_transcript(&mut self) -> Option<Duration> {
        let elapsed = self.ended_uncommitted_at?.elapsed();
        if elapsed < ASR_TRANSCRIPT_DEADLINE {
            return None;
        }
        self.uncommitted = false;
        self.ended_uncommitted_at = None;
        Some(elapsed)
    }

    pub(super) fn report_missing_transcript(&mut self) -> Option<UserAudioRecovery> {
        let elapsed = self.expire_missing_transcript()?;
        overlay::set_listening(false);
        overlay::push_log(
            "(speech ended without a transcript; no command was created)".to_string(),
        );
        telemetry::typed_error(
            "ERR_INPUT_TRANSCRIPT_TIMEOUT",
            "speech",
            "uploaded voice ended but no input transcript arrived before the deadline",
            serde_json::json!({"deadline_ms": ASR_TRANSCRIPT_DEADLINE.as_millis(), "elapsed_ms": elapsed.as_millis()}),
        );
        telemetry::event(
            "user_audio_recovery",
            "speech",
            Privacy::Safe,
            serde_json::json!({
                "state": "transcript_timeout",
                "command_created": false,
                "automatic_retry": false,
            }),
        );
        Some(UserAudioRecovery::TranscriptTimedOut)
    }
}

pub(super) fn audio(state: &mut Reader, pcm: &[i16], sink: Option<&AudioSink>) {
    let utterance_id = *state
        .assistant_utterance_id
        .get_or_insert_with(|| telemetry::next_utterance("assistant_audio"));
    let queued_before = sink.map(AudioSink::queued_samples).unwrap_or(0);
    let queued_now = sink.map(|output| output.push(pcm)).unwrap_or(0);
    let queued_after = sink.map(AudioSink::queued_samples).unwrap_or(0);
    telemetry::event(
        "assistant_audio_chunk",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": utterance_id,
            "received_samples_24k": pcm.len(),
            "queued_samples_before": queued_before,
            "queued_samples_after": queued_after,
            "queued_from_chunk": queued_now,
            "audio_sink_available": sink.is_some(),
        }),
    );
}

pub(super) fn interrupted(state: &mut Reader, sink: Option<&AudioSink>) {
    let dropped_samples = sink.map(AudioSink::clear).unwrap_or(0);
    telemetry::event(
        "assistant_playback_interrupted",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "pending_tool": state.pending.id.clone(),
            "dropped_output_samples": dropped_samples,
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
        }),
    );
}

pub(super) fn transcript(state: &mut Reader, text: &str, _sink: Option<&AudioSink>) {
    let utterance_id = *state
        .assistant_utterance_id
        .get_or_insert_with(|| telemetry::next_utterance("assistant_transcript"));
    telemetry::event(
        "assistant_transcript_delta",
        "speech",
        Privacy::UserText,
        serde_json::json!({
            "utterance_id": utterance_id,
            "char_count": text.chars().count(),
            "preview": text.chars().take(160).collect::<String>(),
        }),
    );
    state.reasoning.push_str(text);
    state.reply.push_str(text);
    overlay::set_model_text(state.reply.clone());
}

pub(super) fn generation_complete(state: &mut Reader, sink: Option<&AudioSink>) {
    telemetry::event(
        "assistant_audio_generation_complete",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
        }),
    );
}

pub(super) fn generation_before_tool(state: &mut Reader, sink: Option<&AudioSink>) {
    telemetry::event(
        "assistant_audio_generation_complete",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::{
        ASR_TRANSCRIPT_DEADLINE, InputTranscriptAssembler, Reader, UserAudioRecovery,
        UserAudioTracker, audio, transcript,
    };
    use std::time::Instant;

    #[test]
    fn uploaded_voice_remains_uncommitted_until_a_transcript_arrives() {
        let mut tracker = UserAudioTracker::default();
        tracker.update(true, 0.5, 960);
        assert!(tracker.has_uncommitted_audio());
        tracker.commit_transcript();
        assert!(!tracker.has_uncommitted_audio());
    }

    #[test]
    fn typed_output_transcript_is_never_content_filtered() {
        let mut state = Reader::default();
        transcript(&mut state, "{\"spoken\":true}", None);
        assert_eq!(state.reply, "{\"spoken\":true}");
        assert_eq!(state.reasoning, state.reply);
    }

    #[test]
    fn typed_audio_is_processed_before_any_transcript() {
        let mut state = Reader::default();
        audio(&mut state, &[1, -1, 2, -2], None);
        assert!(state.assistant_utterance_id.is_some());
        assert!(state.reply.is_empty());
    }

    #[test]
    fn ended_voice_without_transcript_expires_once() {
        let mut tracker = UserAudioTracker {
            uncommitted: true,
            ended_uncommitted_at: Some(Instant::now() - ASR_TRANSCRIPT_DEADLINE),
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
        let duplicate = transcript.merge("with confirmation").unwrap();
        assert!(!duplicate.changed);
    }

    #[test]
    fn input_transcript_resets_only_when_lifecycle_requests_it() {
        let mut transcript = InputTranscriptAssembler::default();
        assert!(transcript.merge("first").unwrap().starts_turn);
        assert!(!transcript.merge("second").unwrap().starts_turn);
        transcript.reset();
        assert!(transcript.merge("new turn").unwrap().starts_turn);
    }
}
