//! Wire-protocol decoding for the Gemini Live websocket.
//!
//! Parses server JSON frames into a flat [`ParsedUpdate`] and applies the
//! decoded transcripts / audio / turn signals. [`PlaybackBridge`] forwards
//! decoded audio chunks into the shared TTS playback queue.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;

pub(super) fn handle_update(
    message: &str,
    hwnd: isize,
    playback: &mut PlaybackBridge,
    owner: &Arc<AtomicBool>,
    lifecycle: &mut crate::api::gemini_live::lifecycle::LiveSessionLifecycle,
    now_ms: u64,
) -> anyhow::Result<()> {
    let update = parse_update(message);
    if update.error.is_some() {
        return Err(super::SessionFailure::Server {
            retryable: update.error_retryable,
        }
        .into());
    }
    if let Ok(frame) = crate::api::gemini_live::server_frame::parse_server_frame(message) {
        let generation = lifecycle.state().generation;
        lifecycle.reduce(
            now_ms,
            crate::api::gemini_live::lifecycle::LiveLifecycleEvent::Frame(
                crate::api::gemini_live::lifecycle::LiveLifecycleFrame::from_server_frame(
                    generation, &frame,
                ),
            ),
        );
    }

    if let Some(text) = update.input_transcript {
        let mut pending = Vec::new();
        super::publish_owned(owner, || {
            pending = super::super::state::upsert_transcript("input", text, update.turn_complete);
        });
        super::maintenance::request_detection(pending);
    }
    if let Some(text) = update.output_transcript {
        let mut pending = Vec::new();
        super::publish_owned(owner, || {
            pending = super::super::state::upsert_transcript("output", text, update.turn_complete);
        });
        super::maintenance::request_detection(pending);
    }
    for audio in update.audio_chunks {
        playback.push(audio);
    }
    if update.interrupted {
        finalize(owner);
        playback.interrupt(hwnd)?;
    } else if update.turn_complete {
        finalize(owner);
    }
    if update.go_away {
        // Server is about to terminate — trigger clean reconnect
        return Err(super::SessionFailure::Close(1001).into());
    }

    Ok(())
}

fn finalize(owner: &Arc<AtomicBool>) {
    let mut pending = Vec::new();
    if super::publish_owned(owner, || {
        pending = super::super::state::finalize_transcripts();
    }) {
        super::maintenance::request_detection(pending);
        super::request_persist();
    }
}

pub(super) struct PlaybackBridge {
    tx: mpsc::Sender<AudioEvent>,
    id: u64,
    stop: Arc<AtomicBool>,
    playback_ids: Arc<Mutex<Vec<u64>>>,
    volume: Arc<super::super::volume::VolumeControl>,
}

impl PlaybackBridge {
    pub(super) fn new(
        hwnd: isize,
        stop: Arc<AtomicBool>,
        playback_ids: Arc<Mutex<Vec<u64>>>,
    ) -> anyhow::Result<Self> {
        let mut ids = playback_ids.lock().unwrap();
        if stop.load(Ordering::SeqCst) {
            anyhow::bail!("Session stopped");
        }
        ids.retain(|id| TTS_MANAGER.is_speaking(*id));
        let (tx, rx) = mpsc::channel();
        let id = TTS_MANAGER.enqueue_audio(rx, hwnd, false);
        ids.push(id);
        drop(ids);
        Ok(Self {
            tx,
            id,
            stop,
            playback_ids,
            volume: super::super::volume::OUTPUT_VOLUME.clone(),
        })
    }

    pub(super) fn push(&self, mut bytes: Vec<u8>) {
        if !self.stop.load(Ordering::SeqCst) {
            self.volume.apply_pcm(&mut bytes);
            let _ = self.tx.send(AudioEvent::Data(bytes));
        }
    }

    pub(super) fn end(&self) {
        let _ = self.tx.send(AudioEvent::End);
    }

    pub(super) fn interrupt(&mut self, hwnd: isize) -> anyhow::Result<()> {
        TTS_MANAGER.stop_if_active(self.id);
        *self = Self::new(hwnd, self.stop.clone(), self.playback_ids.clone())?;
        Ok(())
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
    pub(super) error_retryable: bool,
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
        error_retryable: frame.error_retryable,
        go_away: frame.go_away.is_some(),
    }
}

fn trimmed_non_empty(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_volume_scales_admitted_pcm_without_global_gain_or_owner_changes() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/translation-gummy/volume-control.json"
        ))
        .unwrap();
        let volume = Arc::new(super::super::super::volume::VolumeControl::default());
        let other = super::super::super::volume::VolumeControl::default();
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let bridge = PlaybackBridge {
            tx,
            id: 7,
            stop: stop.clone(),
            playback_ids: Arc::new(Mutex::new(vec![7])),
            volume: volume.clone(),
        };
        let pcm = [-32768i16, -10000, 0, 10000, 32767]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            volume.percent(),
            fixture["defaultVolumePercent"].as_u64().unwrap() as u32
        );
        for (percent, expected) in [
            (100, vec![-32768, -10000, 0, 10000, 32767]),
            (65, vec![-21299, -6500, 0, 6500, 21298]),
            (0, vec![0, 0, 0, 0, 0]),
            (i64::MAX, vec![-32768, -10000, 0, 10000, 32767]),
            (i64::MIN, vec![0, 0, 0, 0, 0]),
        ] {
            volume.set_percent(percent);
            bridge.push(pcm.clone());
            let AudioEvent::Data(bytes) = rx.recv().unwrap() else {
                panic!("expected PCM data")
            };
            let samples = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| i16::from_le_bytes(*pair))
                .collect::<Vec<_>>();
            assert_eq!(samples, expected);
            assert_eq!(*bridge.playback_ids.lock().unwrap(), vec![7]);
            assert_eq!(other.percent(), 100);
            assert!(!stop.load(Ordering::SeqCst));
        }
        stop.store(true, Ordering::SeqCst);
        bridge.push(pcm);
        assert!(rx.try_recv().is_err(), "retired owner cannot enqueue audio");
    }

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

    #[test]
    fn preserves_structured_server_retryability() {
        let temporary = parse_update(r#"{"error":{"code":503,"message":"unavailable"}}"#);
        let permanent = parse_update(r#"{"error":{"code":403,"message":"denied"}}"#);
        assert!(temporary.error_retryable);
        assert!(!permanent.error_retryable);
    }
}
