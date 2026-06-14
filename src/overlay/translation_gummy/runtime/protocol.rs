//! Wire-protocol decoding for the Gemini Live websocket.
//!
//! Parses server JSON frames into a flat [`ParsedUpdate`] and applies the
//! decoded transcripts / audio / turn signals. [`PlaybackBridge`] forwards
//! decoded audio chunks into the shared TTS playback queue.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;
use base64::{Engine as _, engine::general_purpose};

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
    if let Some(audio) = update.audio_chunk {
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

pub(super) struct ParsedUpdate {
    pub(super) setup_complete: bool,
    pub(super) input_transcript: Option<String>,
    pub(super) output_transcript: Option<String>,
    pub(super) audio_chunk: Option<Vec<u8>>,
    pub(super) turn_complete: bool,
    pub(super) interrupted: bool,
    pub(super) error: Option<String>,
    pub(super) go_away: bool,
}

pub(super) fn parse_update(message: &str) -> ParsedUpdate {
    let mut update = ParsedUpdate {
        setup_complete: false,
        input_transcript: None,
        output_transcript: None,
        audio_chunk: None,
        turn_complete: false,
        interrupted: false,
        error: None,
        go_away: false,
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(message) else {
        return update;
    };

    if message.contains("setupComplete") {
        update.setup_complete = true;
    }

    // GoAway: server signals imminent termination — reconnect gracefully
    if json.get("goAway").is_some() {
        update.go_away = true;
        return update;
    }

    if let Some(error) = json.get("error") {
        update.error = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| Some(error.to_string()));
        return update;
    }

    let Some(server_content) = json.get("serverContent") else {
        return update;
    };

    if server_content
        .get("turnComplete")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || server_content
            .get("generationComplete")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    {
        update.turn_complete = true;
    }
    update.interrupted = server_content
        .get("interrupted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    update.input_transcript = server_content
        .get("inputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    update.output_transcript = server_content
        .get("outputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(parts) = server_content
        .get("modelTurn")
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if update.audio_chunk.is_none()
                && let Some(inline) = part.get("inlineData")
                && let Some(data) = inline.get("data").and_then(|value| value.as_str())
                && let Ok(bytes) = general_purpose::STANDARD.decode(data)
            {
                update.audio_chunk = Some(bytes);
            }
        }
    }

    update
}
