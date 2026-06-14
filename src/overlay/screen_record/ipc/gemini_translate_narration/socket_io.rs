use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tungstenite::Message;

use crate::api::realtime_audio::s2s::transport::parse_s2s_update;
use crate::api::realtime_audio::websocket::{
    is_recoverable_socket_error, is_transient_socket_read_error, pcm_bytes_to_i16,
};

use super::output_vad::{OutputRegion, OutputVad};
use super::text_delta::merge_text;
use super::timeline_audio::append_received_audio_on_clock;

const OUTPUT_SAMPLE_RATE: f64 = 24_000.0;

pub(super) type LiveSocket = tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>;

pub(super) fn wait_for_setup(
    socket: &mut LiveSocket,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let started = Instant::now();
    while !cancelled.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(message) => {
                if let Some(text) = message_to_text(message) {
                    let update = parse_s2s_update(&text);
                    if let Some(error) = update.error {
                        return Err(error);
                    }
                    if update.setup_complete {
                        return Ok(());
                    }
                }
            }
            Err(error) if is_transient_socket_read_error(&error) => {
                if started.elapsed() > Duration::from_secs(15) {
                    return Err("Gemini Translate setup timeout".to_string());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Cancelled".to_string())
}

pub(super) fn drain_socket(
    socket: &mut LiveSocket,
    vad: &mut OutputVad,
    full_output: &mut Vec<i16>,
    output_clock: Instant,
    source_text: &mut String,
    target_text: &mut String,
    mut on_region: impl FnMut(OutputRegion, &str, &str, f64) -> Result<(), String>,
) -> Result<DrainOutcome, String> {
    let mut outcome = DrainOutcome::default();
    loop {
        match socket.read() {
            Ok(message) => {
                let Some(text) = message_to_text(message) else {
                    continue;
                };
                let update = parse_s2s_update(&text);
                if let Some(error) = update.error {
                    return Err(error);
                }
                if let Some(text) = update.input_transcript {
                    merge_text(source_text, &text);
                    outcome.had_activity = true;
                }
                if let Some(text) = update.output_transcript {
                    merge_text(target_text, &text);
                    outcome.had_activity = true;
                }
                for bytes in update.audio_chunks {
                    let samples = pcm_bytes_to_i16(&bytes);
                    if samples.is_empty() {
                        continue;
                    }
                    let append =
                        append_received_audio_on_clock(full_output, vad, output_clock, &samples);
                    outcome.audio_chunks += 1;
                    outcome.audio_samples += append.audio_samples;
                    outcome.silence_samples += append.silence_samples;
                    for region in append.regions {
                        on_region(
                            region,
                            source_text,
                            target_text,
                            full_output.len() as f64 / OUTPUT_SAMPLE_RATE,
                        )?;
                    }
                    outcome.had_activity = true;
                }
                if update.turn_complete {
                    outcome.had_activity = true;
                    outcome.turn_complete = true;
                    return Ok(outcome);
                }
            }
            Err(error)
                if is_transient_socket_read_error(&error)
                    || is_recoverable_socket_error(&error) =>
            {
                return Ok(outcome);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[derive(Default)]
pub(super) struct DrainOutcome {
    pub(super) had_activity: bool,
    pub(super) turn_complete: bool,
    pub(super) audio_chunks: usize,
    pub(super) audio_samples: usize,
    pub(super) silence_samples: usize,
}

pub(super) fn decode_wav_mono_i16(bytes: &[u8]) -> Result<Vec<i16>, String> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| format!("Read prepared Gemini Translate WAV: {error}"))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err(format!(
            "Prepared Gemini Translate WAV must be 16 kHz mono, got {} Hz {} channel(s)",
            spec.sample_rate, spec.channels
        ));
    }
    reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Decode prepared Gemini Translate WAV samples: {error}"))
}

fn message_to_text(message: Message) -> Option<String> {
    match message {
        Message::Text(text) => Some(text.to_string()),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
        _ => None,
    }
}
