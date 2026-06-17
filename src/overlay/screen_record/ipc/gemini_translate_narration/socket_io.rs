use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tungstenite::Message;

use crate::api::realtime_audio::s2s::transport::parse_s2s_update;
use crate::api::realtime_audio::websocket::{
    is_recoverable_socket_error, is_transient_socket_read_error, pcm_bytes_to_i16,
};

use super::output_vad::{OutputRegion, OutputVad, samples_have_speech};
use super::text_delta::merge_text;

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
                    // Gemini Live Translate is a CONTINUOUS model: append every
                    // output chunk in arrival order, exactly like the canonical
                    // Live Translate path. Never wall-clock-pad — fabricated
                    // silence between words is what scattered/dropped speech.
                    full_output.extend_from_slice(&samples);
                    outcome.audio_chunks += 1;
                    outcome.audio_samples += samples.len();
                    outcome.had_output_speech |= samples_have_speech(&samples);
                    // The VAD runs only to find subtitle-cue boundaries over the
                    // contiguous audio; it never cuts or gates the audio itself.
                    for region in vad.push(&samples) {
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
    pub(super) had_output_speech: bool,
    pub(super) turn_complete: bool,
    pub(super) audio_chunks: usize,
    pub(super) audio_samples: usize,
}

fn message_to_text(message: Message) -> Option<String> {
    match message {
        Message::Text(text) => Some(text.to_string()),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
        _ => None,
    }
}
