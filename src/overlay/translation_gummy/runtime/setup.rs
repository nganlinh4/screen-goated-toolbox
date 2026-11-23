//! Session setup handshake: builds the Gemini Live `setup` payload, sends it,
//! and waits for `setupComplete` before the main read loop starts.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::APP;
use crate::api::gemini_live::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};
use crate::config::TranslationGummySettings;
use tungstenite::Message;

use super::protocol::parse_update;

pub(super) fn send_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    settings: &TranslationGummySettings,
) -> anyhow::Result<()> {
    let (model_name, voice_name) = current_gemini_tts_settings();
    let payload = build_setup_payload(
        &model_name,
        &voice_name,
        &settings.build_system_instruction(),
    );

    socket.write(Message::Text(payload.to_string().into()))?;
    socket.flush()?;
    Ok(())
}

fn build_setup_payload(
    model_name: &str,
    voice_name: &str,
    system_instruction: &str,
) -> serde_json::Value {
    LiveSetupBuilder::new(model_name)
        .media_resolution(MediaResolution::Low)
        .voice(voice_name)
        .system_instruction(system_instruction)
        .transcription(TranscriptionMode::Both)
        .context_window_compression()
        .setup_field(
            "realtimeInputConfig",
            serde_json::json!({
                "automaticActivityDetection": {
                    "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
                    "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
                    "prefixPaddingMs": 80,
                    "silenceDurationMs": 320
                },
                "activityHandling": "START_OF_ACTIVITY_INTERRUPTS",
                "turnCoverage": "TURN_INCLUDES_ONLY_ACTIVITY"
            }),
        )
        .build()
}

pub(in crate::overlay::translation_gummy) fn current_gemini_tts_settings() -> (String, String) {
    APP.lock()
        .map(|app| {
            let model = app.config.tts_gemini_live_model.trim();
            let voice = app.config.tts_voice.trim();
            (
                crate::model_config::normalize_tts_gemini_model(model).to_string(),
                if voice.is_empty() {
                    "Aoede".to_string()
                } else {
                    voice.to_string()
                },
            )
        })
        .unwrap_or_else(|_| {
            (
                crate::model_config::DEFAULT_GEMINI_LIVE_TTS_MODEL.to_string(),
                "Aoede".to_string(),
            )
        })
}

pub(super) fn wait_for_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<Vec<String>> {
    let started = Instant::now();
    let mut pending = SetupFrames::default();
    while !stop.load(Ordering::SeqCst) {
        if started.elapsed() > Duration::from_secs(15) {
            return Err(anyhow::anyhow!("setup timeout"));
        }
        match socket.read() {
            Ok(Message::Text(msg)) => {
                if pending.push(msg.as_str())? {
                    return Ok(pending.frames);
                }
            }
            Ok(Message::Binary(data)) => {
                pending.check_size(data.len())?;
                if let Ok(text) = String::from_utf8(data.to_vec())
                    && pending.push(&text)?
                {
                    return Ok(pending.frames);
                }
            }
            Ok(Message::Close(frame)) => {
                return Err(super::SessionFailure::Close(
                    frame.map(|f| u16::from(f.code)).unwrap_or(1006),
                )
                .into());
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(anyhow::anyhow!("stopped"))
}

const MAX_SETUP_FRAMES: usize = 64;
const MAX_SETUP_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct SetupFrames {
    frames: Vec<String>,
    bytes: usize,
}

impl SetupFrames {
    fn check_size(&self, len: usize) -> anyhow::Result<usize> {
        let Some(bytes) = self.bytes.checked_add(len) else {
            return Err(super::SessionFailure::SetupOverflow.into());
        };
        if self.frames.len() >= MAX_SETUP_FRAMES || bytes > MAX_SETUP_BYTES {
            return Err(super::SessionFailure::SetupOverflow.into());
        }
        Ok(bytes)
    }

    fn push(&mut self, message: &str) -> anyhow::Result<bool> {
        let bytes = self.check_size(message.len())?;
        let complete = setup_complete(message)?;
        self.bytes = bytes;
        self.frames.push(message.to_string());
        Ok(complete)
    }
}

fn setup_complete(message: &str) -> anyhow::Result<bool> {
    let update = parse_update(message);
    if update.error.is_some() {
        return Err(super::SessionFailure::Server {
            retryable: update.error_retryable,
        }
        .into());
    }
    Ok(update.setup_complete)
}

#[cfg(test)]
mod setup_contract_tests {
    use super::*;

    // Cross-platform parity lock — see `.claude/parity/translation-gummy.md`.
    const FIXTURE: &str =
        include_str!("../../../../parity-fixtures/translation-gummy/vad-contract.json");

    #[test]
    fn setup_frame_retains_content_and_error_precedes_acknowledgement() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/translation-gummy/socket-protocol.json"
        ))
        .unwrap();
        let complete = &fixture["setupComplete"];
        let payload = complete["payload"].to_string();
        assert_eq!(
            setup_complete(&payload).unwrap(),
            complete["expected"]["setupComplete"].as_bool().unwrap()
        );
        assert_eq!(parse_update(&payload).error, None);
        let rejected = &fixture["errorContainingSetupCompleteText"];
        let payload = rejected["payload"].to_string();
        assert!(setup_complete(&payload).is_err());
        assert_eq!(
            parse_update(&payload).error.as_deref(),
            rejected["expected"]["error"].as_str()
        );
        let case = &fixture["setupCompleteWithContent"];
        let ready = case["payload"].to_string();
        assert!(setup_complete(&ready).unwrap());
        let mut pending = SetupFrames::default();
        assert!(pending.push(&ready).unwrap());
        assert_eq!(
            pending.frames.len() as u64,
            case["expected"]["deliveries"].as_u64().unwrap()
        );
        assert_eq!(
            parse_update(&ready).input_transcript.as_deref(),
            case["expected"]["inputTranscript"].as_str()
        );
        let error = fixture["setupCompleteWithError"]["payload"].to_string();
        assert!(setup_complete(&error).is_err());
    }

    #[test]
    fn setup_queue_preserves_order_and_bounds_input() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/translation-gummy/socket-protocol.json"
        ))
        .unwrap();
        assert_eq!(
            MAX_SETUP_FRAMES as u64,
            fixture["setupBuffer"]["maxFrames"].as_u64().unwrap()
        );
        assert_eq!(
            MAX_SETUP_BYTES as u64,
            fixture["setupBuffer"]["maxBytes"].as_u64().unwrap()
        );
        assert_eq!(fixture["setupBuffer"]["overflow"], "fail");
        let case = &fixture["contentBeforeSetupComplete"];
        let mut pending = SetupFrames::default();
        for (index, payload) in case["payloads"].as_array().unwrap().iter().enumerate() {
            assert_eq!(pending.push(&payload.to_string()).unwrap(), index == 1);
        }
        let observed: Vec<_> = pending
            .frames
            .iter()
            .filter_map(|frame| parse_update(frame).input_transcript)
            .collect();
        let expected: Vec<_> = case["expected"]["inputTranscripts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(observed, expected);
        let mut count = SetupFrames::default();
        for _ in 0..MAX_SETUP_FRAMES {
            assert!(!count.push("{}").unwrap());
        }
        assert!(count.push("{}").is_err());
        assert!(
            SetupFrames::default()
                .push(&"x".repeat(MAX_SETUP_BYTES + 1))
                .is_err()
        );
    }

    #[test]
    fn setup_payload_matches_parity_fixture() {
        let doc: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
        let setup_fixture = &doc["setup"];

        let model = crate::model_config::GEMINI_LIVE_API_MODEL_3_1;
        let payload = build_setup_payload(model, "VoiceX", "instruction");
        let setup = &payload["setup"];
        let generation = &setup["generationConfig"];
        let realtime = &setup["realtimeInputConfig"];
        let activity = &realtime["automaticActivityDetection"];

        assert_eq!(
            activity["startOfSpeechSensitivity"].as_str().unwrap(),
            setup_fixture["startSensitivity"].as_str().unwrap(),
        );
        assert_eq!(
            activity["endOfSpeechSensitivity"].as_str().unwrap(),
            setup_fixture["endSensitivity"].as_str().unwrap(),
        );
        assert_eq!(
            activity["prefixPaddingMs"].as_u64().unwrap(),
            setup_fixture["prefixPaddingMs"].as_u64().unwrap(),
        );
        assert_eq!(
            activity["silenceDurationMs"].as_u64().unwrap(),
            setup_fixture["silenceDurationMs"].as_u64().unwrap(),
        );
        assert_eq!(
            generation["thinkingConfig"],
            setup_fixture["thinkingByModel"][model],
        );
        assert_eq!(
            generation["mediaResolution"].as_str().unwrap(),
            setup_fixture["mediaResolution"].as_str().unwrap(),
        );
        assert_eq!(
            realtime["activityHandling"].as_str().unwrap(),
            setup_fixture["activityHandling"].as_str().unwrap(),
        );
        assert_eq!(
            realtime["turnCoverage"].as_str().unwrap(),
            setup_fixture["turnCoverage"].as_str().unwrap(),
        );
    }
}
