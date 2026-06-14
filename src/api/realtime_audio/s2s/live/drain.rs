use super::super::transport::parse_s2s_update;
use super::super::*;
use super::text_state::LiveTranslateTextState;

pub(super) fn drain_live_translate_socket(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stream_id: u64,
    event_tx: &mpsc::Sender<S2sEvent>,
    received_audio_chunks: &mut usize,
    text_state: &mut LiveTranslateTextState,
    last_server_activity: &mut Instant,
    sent_chunks_at_last_activity: &mut usize,
    sent_chunks: usize,
    playback: &crate::api::tts::player::audio_player::AudioPlayer,
) -> Result<bool> {
    loop {
        match socket.read() {
            Ok(Message::Close(frame)) => {
                crate::log_info!(
                    "[RealtimeLiveTranslate] continuous socket closed stream={} frame={:?}",
                    stream_id,
                    frame
                );
                return Ok(false);
            }
            Ok(message) => {
                if let Some(text) = s2s_message_to_text(message) {
                    handle_live_translate_message(
                        stream_id,
                        &text,
                        event_tx,
                        received_audio_chunks,
                        text_state,
                        last_server_activity,
                        sent_chunks_at_last_activity,
                        sent_chunks,
                        playback,
                    );
                }
            }
            Err(error) if is_transient_socket_read_error(&error) => {
                return Ok(true);
            }
            Err(error) if is_recoverable_socket_error(&error) => {
                crate::log_info!(
                    "[RealtimeLiveTranslate] continuous socket recover stream={} error={}",
                    stream_id,
                    error
                );
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn handle_live_translate_message(
    stream_id: u64,
    message: &str,
    event_tx: &mpsc::Sender<S2sEvent>,
    received_audio_chunks: &mut usize,
    text_state: &mut LiveTranslateTextState,
    last_server_activity: &mut Instant,
    sent_chunks_at_last_activity: &mut usize,
    sent_chunks: usize,
    playback: &crate::api::tts::player::audio_player::AudioPlayer,
) {
    let update = parse_s2s_update(message);
    if let Some(error) = update.error {
        let _ = event_tx.send(S2sEvent::Error {
            id: stream_id,
            message: error,
        });
        return;
    }
    if update.interrupted {
        let _ = event_tx.send(S2sEvent::Interrupt);
    }
    let mut text_changed = false;
    if let Some(text) = update.input_transcript {
        text_changed |= text_state.update_source(&text);
    }
    if let Some(text) = update.output_transcript {
        text_changed |= text_state.update_target(&text);
    }
    let audio_chunk_count = update.audio_chunks.len();
    let has_activity =
        text_changed || audio_chunk_count > 0 || update.interrupted || update.turn_complete;
    if has_activity {
        *last_server_activity = Instant::now();
        *sent_chunks_at_last_activity = sent_chunks;
    }
    if text_changed {
        let _ = event_tx.send(text_state.snapshot_event());
    }
    *received_audio_chunks += audio_chunk_count;
    for bytes in update.audio_chunks {
        playback.play_native_stream(&bytes);
    }
}
