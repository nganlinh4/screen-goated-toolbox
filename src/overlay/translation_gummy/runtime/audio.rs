//! Local microphone plumbing and voice-activity detection (VAD).
//!
//! Drains and forwards every captured PCM chunk. Noise-relative activity adds
//! end-of-speech boundaries without discarding quiet input. Constants are the parity contract
//! against the Android runtime — see `.claude/parity/translation-gummy.md`.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::api::realtime_audio::websocket::{send_audio_chunk, send_audio_stream_end};

pub(super) const CHUNK_SAMPLES: usize = 1600;
#[cfg(test)]
pub(super) const LOCAL_INPUT_SPEECH_RMS: f32 = crate::api::audio::activity::MAX_SPEECH_RMS;
pub(super) const LOCAL_INPUT_TRAILING_AUDIO_MS: u64 = 180;
pub(super) const LOCAL_INPUT_END_SILENCE_MS: u64 = 420;

pub(super) struct LocalInputTurnState {
    activity: crate::api::audio::activity::SpeechActivity,
    pub(super) turn_active: bool,
    pub(super) last_speech_at: Option<Instant>,
}

impl LocalInputTurnState {
    pub(super) fn new() -> Self {
        Self {
            activity: Default::default(),
            turn_active: false,
            last_speech_at: None,
        }
    }
}

pub(super) fn flush_audio(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
    pending_audio: &mut Vec<i16>,
    input_turn: &mut LocalInputTurnState,
) -> anyhow::Result<()> {
    {
        let mut guard = buffer.lock().unwrap();
        if !guard.is_empty() {
            pending_audio.extend(guard.drain(..));
        }
    }

    while pending_audio.len() >= CHUNK_SAMPLES {
        let chunk: Vec<i16> = pending_audio.drain(..CHUNK_SAMPLES).collect();
        let rms = calculate_rms(&chunk);
        super::super::publish_audio_level(calculate_audio_level(&chunk));
        if input_turn.activity.observe(rms, Instant::now()) {
            if !input_turn.turn_active {
                input_turn.turn_active = true;
            }
            input_turn.last_speech_at = Some(Instant::now());
            send_audio_chunk(socket, &chunk)?;
            continue;
        }

        if !input_turn.turn_active {
            // Provider VAD receives quiet input too; local RMS only adds boundaries.
            send_audio_chunk(socket, &chunk)?;
            continue;
        }

        let silence_ms = input_turn
            .last_speech_at
            .map(|started| started.elapsed().as_millis() as u64)
            .unwrap_or(LOCAL_INPUT_END_SILENCE_MS);
        send_audio_chunk(socket, &chunk)?;
        if silence_ms <= LOCAL_INPUT_TRAILING_AUDIO_MS {
            continue;
        }
        if silence_ms >= LOCAL_INPUT_END_SILENCE_MS {
            send_audio_stream_end(socket)?;
            input_turn.turn_active = false;
            input_turn.last_speech_at = None;
        }
    }
    Ok(())
}

fn calculate_audio_level(samples: &[i16]) -> f32 {
    (calculate_rms(samples) * 5.5).clamp(0.0, 1.0)
}

fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod vad_contract_tests {
    use super::*;

    // Cross-platform parity lock. Rust is canonical; the Android (Kotlin) runtime
    // asserts the same file so the VAD + setup constants cannot drift.
    // See .claude/parity/translation-gummy.md.
    const FIXTURE: &str =
        include_str!("../../../../parity-fixtures/translation-gummy/vad-contract.json");

    #[test]
    fn vad_constants_match_parity_fixture() {
        let doc: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
        let vad = &doc["vad"];

        assert_eq!(
            vad["speechRms"].as_f64().expect("speechRms") as f32,
            LOCAL_INPUT_SPEECH_RMS,
            "speech RMS threshold drifted from fixture",
        );
        assert_eq!(
            vad["trailingAudioMs"].as_u64().expect("trailingAudioMs"),
            LOCAL_INPUT_TRAILING_AUDIO_MS,
            "trailing-audio grace window drifted from fixture",
        );
        assert_eq!(
            vad["endSilenceMs"].as_u64().expect("endSilenceMs"),
            LOCAL_INPUT_END_SILENCE_MS,
            "end-of-speech silence window drifted from fixture",
        );

        // Normal capture is continuous; Android retains pre-roll only for barge-in.
        assert_eq!(
            vad["quietInputPolicy"],
            "forward_continuously_local_activity_only_adds_boundaries"
        );
        assert_eq!(
            CHUNK_SAMPLES as u64,
            vad["_chunkSamplesWindows"]
                .as_u64()
                .expect("_chunkSamplesWindows"),
            "Windows chunk size drifted from fixture",
        );
        assert_eq!(
            0,
            vad["_prerollSamplesWindows"]
                .as_u64()
                .expect("_prerollSamplesWindows"),
            "Windows pre-roll samples drifted from fixture",
        );
    }
}
