//! Correlated assistant speech and playback telemetry.

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::telemetry::{self, Privacy};
use super::reader::Reader;
use std::time::{Duration, Instant};

const ASR_TRANSCRIPT_DEADLINE: Duration = Duration::from_secs(5);
const LOW_CONFIDENCE_RETIRE_DEADLINE: Duration = Duration::from_millis(1500);
const MIN_TIMEOUT_ACTIVITY: Duration = Duration::from_millis(750);
const MIN_TIMEOUT_PEAK_LEVEL: f32 = 0.10;
const MIN_TRANSCRIPT_OVERLAP_CHARS: usize = 3;

/// PCM retained only while no output sink exists. With a healthy sink, current
/// generation audio streams immediately; structural ownership rejects late
/// generations and interruption clears samples that have not played yet.
#[derive(Default)]
pub(super) struct GenerationAudioBuffer {
    samples_24k: Vec<i16>,
}

impl GenerationAudioBuffer {
    fn push(&mut self, pcm: &[i16]) {
        self.samples_24k.extend_from_slice(pcm);
    }

    fn take(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.samples_24k)
    }

    pub(super) fn len(&self) -> usize {
        self.samples_24k.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputTranscriptUpdate {
    pub(super) text: String,
    pub(super) starts_turn: bool,
    pub(super) changed: bool,
    pub(super) fragment_index: usize,
}

/// Joins Live input-transcription deltas within one user turn. The endpoint may
/// send a growing cumulative string, overlapping chunks, or exact duplicates.
/// Lifecycle code resets this only at a structural user/model turn boundary.
#[derive(Default)]
pub(super) struct InputTranscriptAssembler {
    text: String,
    open: bool,
    /// Set by a structural user-input signal (local speech onset, interruption,
    /// or explicit text submission). A transcript without this signal while the
    /// previous epoch is still open is a late revision, not a new request.
    fresh_epoch: bool,
    fragment_count: usize,
}

impl InputTranscriptAssembler {
    pub(super) fn merge(&mut self, fragment: &str) -> Option<InputTranscriptUpdate> {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            return None;
        }
        let starts_turn = self.fresh_epoch || !self.open;
        if starts_turn {
            self.text.clear();
            self.text.push_str(fragment);
            self.open = true;
            self.fresh_epoch = false;
            self.fragment_count = 1;
            return Some(InputTranscriptUpdate {
                text: self.text.clone(),
                starts_turn,
                changed: true,
                fragment_index: self.fragment_count,
            });
        }
        self.fragment_count = self.fragment_count.saturating_add(1);
        let before = self.text.clone();
        merge_transcript_text(&mut self.text, fragment);
        Some(InputTranscriptUpdate {
            text: self.text.clone(),
            starts_turn,
            changed: self.text != before,
            fragment_index: self.fragment_count,
        })
    }

    pub(super) fn begin_epoch(&mut self) {
        self.fresh_epoch = true;
    }

    pub(super) fn has_fresh_epoch(&self) -> bool {
        self.fresh_epoch
    }

    pub(super) fn is_open(&self) -> bool {
        self.open
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
    LowConfidenceActivityExpired,
}

#[derive(Default)]
pub(super) struct PlaybackTracker {
    epoch: u64,
    was_playing: bool,
    quiet_since: Option<Instant>,
    started_at_sample: Option<u64>,
    started_utterance: Option<u64>,
}

impl PlaybackTracker {
    pub(super) fn update(&mut self, playing: bool, state: &mut Reader, sink: Option<&AudioSink>) {
        if self.epoch != state.playback_epoch {
            self.epoch = state.playback_epoch;
            self.was_playing = false;
            self.quiet_since = None;
            self.started_at_sample = None;
            self.started_utterance = None;
        }
        if playing && !self.was_playing {
            let utterance = state.assistant_utterance_id;
            if self.register_start(utterance) {
                self.started_at_sample = sink.map(AudioSink::played_samples);
                telemetry::event(
                    "assistant_playback_started",
                    "speech",
                    Privacy::Safe,
                    serde_json::json!({"utterance_id": utterance}),
                );
            }
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
            let utterance = self.started_utterance;
            if utterance.is_some() {
                telemetry::event(
                    "assistant_playback_completed",
                    "speech",
                    Privacy::Safe,
                    serde_json::json!({
                        "utterance_id": utterance,
                        "playback_epoch": self.epoch,
                        "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
                        "played_output_samples": sink.map(AudioSink::played_samples).unwrap_or(0)
                            .saturating_sub(self.started_at_sample.unwrap_or(0)),
                    }),
                );
            }
            if state.assistant_utterance_id == utterance {
                state.assistant_utterance_id = None;
            }
            self.started_at_sample = None;
            self.started_utterance = None;
            self.quiet_since = None;
        }
    }

    fn register_start(&mut self, utterance: Option<u64>) -> bool {
        if self.started_utterance == utterance {
            return false;
        }
        self.started_utterance = utterance;
        true
    }
}

#[derive(Default)]
pub(super) struct UserAudioTracker {
    active_since: Option<Instant>,
    last_active: Option<Instant>,
    uncommitted: bool,
    ended_uncommitted_at: Option<Instant>,
    peak_level: f32,
    voiced_samples: usize,
    last_evidence: Option<UserAudioEvidence>,
}

#[derive(Clone, Copy, Debug, Default)]
struct UserAudioEvidence {
    duration_ms: u128,
    peak_level: f32,
    voiced_samples: usize,
}

impl UserAudioEvidence {
    fn warrants_timeout(self) -> bool {
        self.duration_ms >= MIN_TIMEOUT_ACTIVITY.as_millis()
            || self.peak_level >= MIN_TIMEOUT_PEAK_LEVEL
    }
}

impl UserAudioTracker {
    pub(super) fn update(&mut self, voiced: bool, level: f32, samples: usize) -> bool {
        let started = voiced && self.active_since.is_none();
        if started {
            self.active_since = Some(Instant::now());
            self.last_active = self.active_since;
            self.uncommitted = true;
            self.ended_uncommitted_at = None;
            self.peak_level = level;
            self.voiced_samples = samples;
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
            self.peak_level = self.peak_level.max(level);
            self.voiced_samples = self.voiced_samples.saturating_add(samples);
        } else if self
            .last_active
            .is_some_and(|last| last.elapsed() > Duration::from_millis(500))
            && let Some(started) = self.active_since.take()
        {
            let duration_ms = started.elapsed().as_millis();
            let evidence = UserAudioEvidence {
                duration_ms,
                peak_level: self.peak_level,
                voiced_samples: self.voiced_samples,
            };
            telemetry::event(
                "user_audio_activity_ended",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "detector": "rms_threshold",
                    "duration_ms": duration_ms,
                    "peak_level": evidence.peak_level,
                    "voiced_samples": evidence.voiced_samples,
                    "timeout_evidence": evidence.warrants_timeout(),
                }),
            );
            overlay::set_listening(false);
            self.last_evidence = Some(evidence);
            self.peak_level = 0.0;
            self.voiced_samples = 0;
            self.last_active = None;
            if self.uncommitted {
                self.ended_uncommitted_at = Some(Instant::now());
            }
        }
        started
    }

    /// Local RMS can hear the assistant's own speakers. During playback, only
    /// the server's typed `Interrupted` event may establish a fresh user epoch.
    pub(super) fn update_for_local_epoch(
        &mut self,
        voiced: bool,
        level: f32,
        samples: usize,
        assistant_playing: bool,
    ) -> bool {
        self.update(voiced, level, samples) && !assistant_playing
    }

    pub(super) fn commit_transcript(&mut self, source: &'static str) {
        let had_pending_audio = self.uncommitted;
        let active_duration_ms = if had_pending_audio {
            self.active_since
                .map(|started| started.elapsed().as_millis())
                .or_else(|| self.last_evidence.map(|evidence| evidence.duration_ms))
        } else {
            None
        };
        let after_audio_end_ms = self
            .ended_uncommitted_at
            .map(|ended| ended.elapsed().as_millis());
        self.uncommitted = false;
        self.ended_uncommitted_at = None;
        self.last_evidence = None;
        telemetry::event(
            "input_transcript_committed",
            "speech",
            Privacy::Safe,
            serde_json::json!({
                "source": source,
                "had_pending_audio": had_pending_audio,
                "audio_activity_duration_ms": active_duration_ms,
                "transcript_after_audio_end_ms": after_audio_end_ms,
                "endpoint_reason": "provider_unspecified",
                "finality": "provider_unspecified",
            }),
        );
    }

    pub(super) fn has_uncommitted_audio(&self) -> bool {
        self.uncommitted
    }

    fn expire_missing_transcript(&mut self) -> Option<(Duration, UserAudioEvidence)> {
        let elapsed = self.ended_uncommitted_at?.elapsed();
        let evidence = self.last_evidence.unwrap_or_default();
        let deadline = if evidence.warrants_timeout() {
            ASR_TRANSCRIPT_DEADLINE
        } else {
            LOW_CONFIDENCE_RETIRE_DEADLINE
        };
        if elapsed < deadline {
            return None;
        }
        self.uncommitted = false;
        self.ended_uncommitted_at = None;
        self.last_evidence = None;
        Some((elapsed, evidence))
    }

    pub(super) fn report_missing_transcript(&mut self) -> Option<UserAudioRecovery> {
        let (elapsed, evidence) = self.expire_missing_transcript()?;
        if !evidence.warrants_timeout() {
            telemetry::event(
                "user_audio_activity_untranscribed",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "state": "low_confidence_activity_expired",
                    "command_created": false,
                    "duration_ms": evidence.duration_ms,
                    "peak_level": evidence.peak_level,
                    "voiced_samples": evidence.voiced_samples,
                    "elapsed_ms": elapsed.as_millis(),
                }),
            );
            return Some(UserAudioRecovery::LowConfidenceActivityExpired);
        }
        overlay::set_listening(false);
        overlay::push_log(
            "(speech ended without a transcript; no command was created)".to_string(),
        );
        telemetry::typed_error(
            "ERR_INPUT_TRANSCRIPT_TIMEOUT",
            "speech",
            "uploaded voice ended but no input transcript arrived before the deadline",
            serde_json::json!({
                "deadline_ms": ASR_TRANSCRIPT_DEADLINE.as_millis(),
                "elapsed_ms": elapsed.as_millis(),
                "audio_activity_duration_ms": evidence.duration_ms,
                "peak_level": evidence.peak_level,
                "voiced_samples": evidence.voiced_samples,
            }),
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
    if !pcm.is_empty() {
        state.generation_output_seen = true;
    }
    let utterance_id = *state
        .assistant_utterance_id
        .get_or_insert_with(|| telemetry::next_utterance("assistant_audio"));
    let queued_before = sink.map(AudioSink::queued_samples).unwrap_or(0);
    let (queued_now, held_for_sink_recovery) = if let Some(output) = sink {
        (output.push(pcm), false)
    } else {
        state.generation_audio.push(pcm);
        (0, !pcm.is_empty())
    };
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
            "held_samples_24k": state.generation_audio.len(),
            "held_for_generation_outcome": false,
            "held_for_sink_recovery": held_for_sink_recovery,
            "audio_sink_available": sink.is_some(),
        }),
    );
}

pub(super) fn release_generation_audio(
    state: &mut Reader,
    sink: Option<&AudioSink>,
    reason: &'static str,
) {
    let pcm = state.generation_audio.take();
    let (queued, flushed_startup) = sink
        .map(|output| {
            (
                output.push_complete_utterance(&pcm),
                output.finish_utterance(),
            )
        })
        .unwrap_or((0, 0));
    telemetry::event(
        "assistant_generation_audio_released",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "reason": reason,
            "released_samples_24k": pcm.len(),
            "queued_output_samples": queued,
            "flushed_startup_samples": flushed_startup,
            "audio_sink_available": sink.is_some(),
        }),
    );
}

pub(super) fn discard_generation_audio(state: &mut Reader, reason: &str) {
    let dropped = state.generation_audio.take().len();
    telemetry::event(
        "assistant_generation_audio_discarded",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "reason": reason,
            "dropped_samples_24k": dropped,
        }),
    );
}

/// Retire the caption and any sink-recovery audio for a failed terminal claim.
/// PCM already streamed to a healthy sink cannot be retracted; structural done
/// failures therefore continue through the normal corrective-response path.
pub(super) fn discard_failed_completion(
    state: &mut Reader,
    sink: Option<&AudioSink>,
    reason: &str,
) {
    let discarded_samples = state.generation_audio.len();
    let discarded_chars = state.reply.chars().count();
    discard_generation_audio(state, reason);
    state.reply.clear();
    state.reply_utterance_id = None;
    overlay::clear_model_caption();
    if !sink.is_some_and(AudioSink::is_playing) && (discarded_samples > 0 || discarded_chars > 0) {
        state.assistant_utterance_id = None;
    }
    telemetry::event(
        "assistant_failed_completion_discarded",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "discarded_chars": discarded_chars,
            "discarded_samples_24k": discarded_samples,
            "reason": reason,
        }),
    );
}

pub(super) fn interrupted(state: &mut Reader, sink: Option<&AudioSink>) {
    let utterance_id = state.assistant_utterance_id.take();
    state.playback_epoch = state.playback_epoch.saturating_add(1);
    let held_samples = state.generation_audio.take().len();
    let dropped_samples = sink.map(AudioSink::clear).unwrap_or(0);
    telemetry::event(
        "assistant_playback_interrupted",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": utterance_id,
            "playback_epoch": state.playback_epoch,
            "pending_tool": state.pending.id.clone(),
            "dropped_output_samples": dropped_samples,
            "dropped_held_samples_24k": held_samples,
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
        }),
    );
}

pub(super) fn transcript(state: &mut Reader, text: &str, _sink: Option<&AudioSink>) {
    if !text.trim().is_empty() {
        state.generation_output_seen = true;
    }
    let utterance_id = *state
        .assistant_utterance_id
        .get_or_insert_with(|| telemetry::next_utterance("assistant_transcript"));
    state.reply_utterance_id = Some(utterance_id);
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
    let flushed_startup = sink.map(AudioSink::finish_utterance).unwrap_or(0);
    telemetry::event(
        "assistant_audio_generation_complete",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
            "flushed_startup_samples": flushed_startup,
        }),
    );
}

pub(super) fn turn_complete(state: &mut Reader, sink: Option<&AudioSink>) {
    telemetry::event(
        "assistant_turn_complete",
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
#[path = "speech_events_tests.rs"]
mod tests;
