//! Local microphone plumbing and voice-activity detection (VAD).
//!
//! Drains captured PCM, runs a simple RMS-gated turn detector (pre-roll +
//! trailing-audio grace + end-of-speech silence) and streams qualifying chunks
//! to the websocket. The VAD constants here are the canonical parity contract
//! against the Android runtime — see `.claude/parity/translation-gummy.md`.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::api::realtime_audio::websocket::{send_audio_chunk, send_audio_stream_end};

pub(super) const CHUNK_SAMPLES: usize = 1600;
pub(super) const LOCAL_INPUT_SPEECH_RMS: f32 = 0.015;
pub(super) const LOCAL_INPUT_TRAILING_AUDIO_MS: u64 = 180;
pub(super) const LOCAL_INPUT_END_SILENCE_MS: u64 = 420;
pub(super) const LOCAL_INPUT_PREROLL_SAMPLES: usize = 3200;

pub(super) struct LocalInputTurnState {
    pub(super) pre_roll: Vec<i16>,
    pub(super) turn_active: bool,
    pub(super) last_speech_at: Option<Instant>,
}

impl LocalInputTurnState {
    pub(super) fn new() -> Self {
        Self {
            pre_roll: Vec::new(),
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
        if rms >= LOCAL_INPUT_SPEECH_RMS {
            if !input_turn.turn_active {
                if !input_turn.pre_roll.is_empty() {
                    send_audio_chunk(socket, &input_turn.pre_roll)?;
                    input_turn.pre_roll.clear();
                }
                input_turn.turn_active = true;
            }
            input_turn.last_speech_at = Some(Instant::now());
            send_audio_chunk(socket, &chunk)?;
            continue;
        }

        if !input_turn.turn_active {
            input_turn.pre_roll.extend_from_slice(&chunk);
            if input_turn.pre_roll.len() > LOCAL_INPUT_PREROLL_SAMPLES {
                let overflow = input_turn.pre_roll.len() - LOCAL_INPUT_PREROLL_SAMPLES;
                input_turn.pre_roll.drain(..overflow);
            }
            continue;
        }

        let silence_ms = input_turn
            .last_speech_at
            .map(|started| started.elapsed().as_millis() as u64)
            .unwrap_or(LOCAL_INPUT_END_SILENCE_MS);
        if silence_ms <= LOCAL_INPUT_TRAILING_AUDIO_MS {
            send_audio_chunk(socket, &chunk)?;
            continue;
        }
        if silence_ms >= LOCAL_INPUT_END_SILENCE_MS {
            send_audio_stream_end(socket)?;
            input_turn.turn_active = false;
            input_turn.last_speech_at = None;
            input_turn.pre_roll.clear();
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

        // Windows expresses pre-roll in samples; the fixture locks the chunk count.
        // Android (device-dependent chunk size) asserts the chunk count directly.
        let preroll_chunks = vad["prerollChunks"].as_u64().expect("prerollChunks") as usize;
        assert_eq!(
            LOCAL_INPUT_PREROLL_SAMPLES,
            preroll_chunks * CHUNK_SAMPLES,
            "Windows pre-roll samples must equal prerollChunks * CHUNK_SAMPLES",
        );
        assert_eq!(
            CHUNK_SAMPLES as u64,
            vad["_chunkSamplesWindows"]
                .as_u64()
                .expect("_chunkSamplesWindows"),
            "Windows chunk size drifted from fixture",
        );
        assert_eq!(
            LOCAL_INPUT_PREROLL_SAMPLES as u64,
            vad["_prerollSamplesWindows"]
                .as_u64()
                .expect("_prerollSamplesWindows"),
            "Windows pre-roll samples drifted from fixture",
        );
    }
}
