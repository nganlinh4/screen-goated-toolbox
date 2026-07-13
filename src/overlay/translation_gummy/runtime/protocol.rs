//! Wire-protocol decoding for the Gemini Live websocket.
//!
//! Parses server JSON frames into a flat [`ParsedUpdate`] and applies the
//! decoded transcripts / audio / turn signals. [`PlaybackBridge`] forwards
//! decoded audio chunks into the shared TTS playback queue.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;

static PLAYBACK_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn handle_update(
    message: &str,
    hwnd: isize,
    playback: &mut PlaybackBridge,
) -> anyhow::Result<()> {
    let update = parse_update(message);
    if let Some(error) = update.error {
        return Err(anyhow::anyhow!(error));
    }

    if let Some(text) = update.input_transcript {
        super::super::upsert_transcript("input", text, update.turn_complete);
    }
    if let Some(text) = update.output_transcript {
        super::super::upsert_transcript("output", text, update.turn_complete);
    }
    for audio in update.audio_chunks {
        playback.push(audio);
    }
    if update.interrupted {
        super::super::finalize_transcripts();
        playback.interrupt(hwnd);
    } else if update.turn_complete {
        super::super::finalize_transcripts();
    }
    if update.go_away {
        // Server is about to terminate — trigger clean reconnect
        return Err(anyhow::anyhow!("connection closed (1001)"));
    }

    Ok(())
}

pub(super) struct PlaybackBridge {
    tx: mpsc::Sender<AudioEvent>,
}

impl PlaybackBridge {
    pub(super) fn new(hwnd: isize) -> Self {
        let (tx, rx) = mpsc::channel();
        let generation = TTS_MANAGER.interrupt_generation.load(Ordering::SeqCst);
        let request_id = PLAYBACK_COUNTER.fetch_add(1, Ordering::SeqCst);
        {
            let mut queue = TTS_MANAGER.playback_queue.lock().unwrap();
            queue.push_back((rx, hwnd, request_id, generation, false));
        }
        TTS_MANAGER.playback_signal.notify_one();
        Self { tx }
    }

    pub(super) fn push(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(AudioEvent::Data(bytes));
    }

    pub(super) fn end(&self) {
        let _ = self.tx.send(AudioEvent::End);
    }

    pub(super) fn interrupt(&mut self, hwnd: isize) {
        TTS_MANAGER.stop();
        *self = Self::new(hwnd);
    }
}

#[derive(Default)]
pub(super) struct ParsedUpdate {
    pub(super) setup_complete: bool,
    pub(super) input_transcript: Option<String>,
    pub(super) output_transcript: Option<String>,
    pub(super) audio_chunks: Vec<Vec<u8>>,
    pub(super) turn_complete: bool,
    pub(super) interrupted: bool,
    pub(super) error: Option<String>,
    pub(super) go_away: bool,
}

pub(super) fn parse_update(message: &str) -> ParsedUpdate {
    let Ok(frame) = crate::api::gemini_live::server_frame::parse_server_frame(message) else {
        return ParsedUpdate::default();
    };
    let turn_complete = frame.response_complete();
    ParsedUpdate {
        setup_complete: frame.setup_complete,
        input_transcript: trimmed_non_empty(frame.input_transcript),
        output_transcript: trimmed_non_empty(frame.output_transcript),
        audio_chunks: frame.audio_chunks,
        turn_complete,
        interrupted: frame.interrupted,
        error: frame.error,
        go_away: frame.go_away.is_some(),
    }
}

fn trimmed_non_empty(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::parse_update;

    #[test]
    fn preserves_every_audio_part_and_generation_completion() {
        let update = parse_update(
            r#"{"serverContent":{"modelTurn":{"parts":[
                {"inlineData":{"data":"AQI="}},
                {"inlineData":{"data":"AwQ="}}
            ]},"generationComplete":true}}"#,
        );

        assert_eq!(update.audio_chunks, vec![vec![1, 2], vec![3, 4]]);
        assert!(update.turn_complete);
    }

    #[test]
    fn does_not_accept_setup_completion_inside_error_text() {
        let update = parse_update(r#"{"error":{"message":"setupComplete failed"}}"#);

        assert!(!update.setup_complete);
        assert_eq!(update.error.as_deref(), Some("setupComplete failed"));
    }
}
