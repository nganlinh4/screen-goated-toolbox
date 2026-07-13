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
) -> anyhow::Result<()> {
    let started = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let update = parse_update(msg.as_str());
                if let Some(error) = update.error {
                    return Err(anyhow::anyhow!(error));
                }
                if update.setup_complete {
                    return Ok(());
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let update = parse_update(&text);
                    if let Some(error) = update.error {
                        return Err(anyhow::anyhow!(error));
                    }
                    if update.setup_complete {
                        return Ok(());
                    }
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if started.elapsed() > Duration::from_secs(15) {
                    return Err(anyhow::anyhow!("setup timeout"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(anyhow::anyhow!("stopped"))
}

#[cfg(test)]
mod setup_contract_tests {
    use super::*;

    // Cross-platform parity lock — see `.claude/parity/translation-gummy.md`.
    const FIXTURE: &str =
        include_str!("../../../../parity-fixtures/translation-gummy/vad-contract.json");

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
