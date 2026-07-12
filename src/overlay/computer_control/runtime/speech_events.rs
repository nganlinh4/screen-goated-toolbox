//! Correlated assistant speech and playback telemetry.

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::telemetry::{self, Privacy};
use super::reader::Reader;
use super::speech_gate::TranscriptDecision;
use std::time::{Duration, Instant};

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
}

impl UserAudioTracker {
    pub(super) fn update(&mut self, voiced: bool, level: f32, samples: usize) {
        if voiced && self.active_since.is_none() {
            self.active_since = Some(Instant::now());
            self.last_active = self.active_since;
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
        }
    }
}

pub(super) fn audio(state: &mut Reader, pcm: &[i16], sink: Option<&AudioSink>) {
    let utterance_id = *state
        .assistant_utterance_id
        .get_or_insert_with(|| telemetry::next_utterance("assistant_audio"));
    let queued_before = sink.map(AudioSink::queued_samples).unwrap_or(0);
    state.speech_gate.push_audio(pcm, sink);
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
            "gate_buffered_or_blocked": queued_after == queued_before,
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

pub(super) fn transcript(state: &mut Reader, text: &str, sink: Option<&AudioSink>) {
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
    match state.speech_gate.transcript(text, sink) {
        TranscriptDecision::Allow(text) => {
            state.reasoning.push_str(text);
            state.reply.push_str(text);
            if !state.speech_gate.is_deferred() {
                overlay::set_model_text(state.reply.clone());
            }
        }
        TranscriptDecision::Block => {
            state.reasoning.clear();
            state.reply.clear();
            overlay::set_model_text(String::new());
            overlay::push_log("[speech-filter] suppressed internal/tool-plan speech".to_string());
            telemetry::event(
                "assistant_speech_blocked",
                "speech",
                Privacy::Safe,
                serde_json::json!({"utterance_id": utterance_id}),
            );
        }
    }
}

pub(super) fn generation_complete(state: &mut Reader, sink: Option<&AudioSink>) {
    let was_deferred = state.speech_gate.is_deferred();
    state.speech_gate.finish_turn(sink);
    if was_deferred {
        overlay::set_model_text(state.reply.clone());
    }
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

pub(super) fn generation_before_tool(state: &mut Reader, sink: Option<&AudioSink>) -> bool {
    let suppressed = state.speech_gate.finish_before_tool(sink);
    if suppressed {
        state.reply.clear();
        overlay::set_model_text(String::new());
        overlay::push_log(
            "[speech-filter] suppressed draft answer before evidence tool".to_string(),
        );
        telemetry::event(
            "assistant_pre_tool_speech_suppressed",
            "speech",
            Privacy::Safe,
            serde_json::json!({"utterance_id": state.assistant_utterance_id}),
        );
    }
    telemetry::event(
        "assistant_audio_generation_complete",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "utterance_id": state.assistant_utterance_id,
            "queued_output_samples": sink.map(AudioSink::queued_samples).unwrap_or(0),
            "played_output_samples_total": sink.map(AudioSink::played_samples).unwrap_or(0),
            "pre_tool_suppressed": suppressed,
        }),
    );
    suppressed
}
