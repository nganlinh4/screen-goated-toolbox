//! Audio messages, setup adapters, and transcription helpers for Gemini Live.

use anyhow::Result;
use std::net::TcpStream;

/// Send session setup message to configure transcription mode
pub fn send_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    model: &str,
) -> Result<()> {
    let setup = crate::api::gemini_live::setup::LiveSetupBuilder::new(model)
        .media_resolution(crate::api::gemini_live::setup::MediaResolution::Low)
        .transcription(crate::api::gemini_live::setup::TranscriptionMode::Input)
        .build();

    let msg_str = setup.to_string();
    socket.write(tungstenite::Message::Text(msg_str.into()))?;
    socket.flush()?;

    Ok(())
}

/// Build the Gemini Live Translate session-setup payload. Shared by the realtime
/// send path and the S2S transport so the translationConfig contract
/// (echoTargetLanguage + in/out audio transcription) lives in one place.
pub fn build_live_translate_setup_value(model: &str, target_language: &str) -> serde_json::Value {
    crate::api::gemini_live::setup::LiveSetupBuilder::new(model)
        .generation_field(
            "translationConfig",
            serde_json::json!({
                "targetLanguageCode": live_translate_target_language_code(target_language),
                "echoTargetLanguage": true
            }),
        )
        .transcription(crate::api::gemini_live::setup::TranscriptionMode::Both)
        .build()
}

/// Send session setup for Gemini Live Translate models.
pub fn send_live_translate_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    model: &str,
    target_language: &str,
) -> Result<()> {
    let setup = build_live_translate_setup_value(model, target_language);

    socket.write(tungstenite::Message::Text(setup.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

pub fn send_audio_stream_end(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
) -> Result<()> {
    let msg = crate::api::gemini_live::client_message::audio_stream_end();

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

/// Send audio chunk to the WebSocket
pub fn send_audio_chunk(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    pcm_data: &[i16],
) -> Result<()> {
    let msg = crate::api::gemini_live::client_message::realtime_audio_pcm(pcm_data, 16_000);

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

/// Decode little-endian PCM16 bytes into i16 samples. Canonical helper shared by
/// the S2S batch pipeline and the Gemini Translate narration socket reader.
pub fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

/// Parse inputTranscription from WebSocket message (what the user said)
pub fn parse_input_transcription(msg: &str) -> Option<String> {
    crate::api::gemini_live::server_frame::parse_server_frame(msg)
        .ok()?
        .input_transcript
}

/// Parse outputTranscription from WebSocket message (what the model said).
pub fn parse_output_transcription(msg: &str) -> Option<String> {
    crate::api::gemini_live::server_frame::parse_server_frame(msg)
        .ok()?
        .output_transcript
}

pub fn live_translate_target_language_code(language: &str) -> String {
    let trimmed = language.trim();
    if trimmed.is_empty() {
        return "en".to_string();
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "chinese"
        | "chinese (simplified)"
        | "simplified chinese"
        | "zh"
        | "zh-cn"
        | "zh-hans"
        | "zh_hans" => return "zh-Hans".to_string(),
        "chinese (traditional)" | "traditional chinese" | "zh-tw" | "zh-hant" | "zh_hant" => {
            return "zh-Hant".to_string();
        }
        "portuguese (brazil)" | "brazilian portuguese" | "pt-br" | "pt_br" => {
            return "pt-BR".to_string();
        }
        "portuguese (portugal)" | "european portuguese" | "pt-pt" | "pt_pt" => {
            return "pt-PT".to_string();
        }
        "filipino" | "tagalog" => return "fil".to_string(),
        "norwegian" => return "no".to_string(),
        code if is_bcp47_like(code) => return normalize_bcp47_code(trimmed),
        _ => {}
    }

    isolang::Language::from_name(trimmed)
        .map(|language| language.to_639_1().unwrap_or_else(|| language.to_639_3()))
        .map(str::to_string)
        .unwrap_or_else(|| "en".to_string())
}

fn is_bcp47_like(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    language.len() >= 2
        && language.len() <= 3
        && language.chars().all(|ch| ch.is_ascii_lowercase())
        && parts.all(|part| {
            !part.is_empty()
                && part.len() <= 8
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

fn normalize_bcp47_code(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh-hans" => "zh-Hans".to_string(),
        "zh-hant" => "zh-Hant".to_string(),
        "pt-br" => "pt-BR".to_string(),
        "pt-pt" => "pt-PT".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::live_translate_target_language_code;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Doc {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        input: String,
        expect: String,
    }

    /// Lock the explicit target-language special cases against the shared fixture
    /// the Android side asserts too. See .claude/parity/gemini-s2s-vad.md.
    #[test]
    fn target_language_codes_match_parity_fixture() {
        let doc: Doc = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/gemini-s2s-language/target-language-codes.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");
        for c in doc.cases {
            assert_eq!(
                live_translate_target_language_code(&c.input),
                c.expect,
                "input {:?}",
                c.input
            );
        }
    }

    /// Lock the Gemini live-translate setup-payload wire format against the shared
    /// fixture the Android side asserts too (structural JSON compare).
    #[test]
    fn live_translate_setup_payload_matches_parity_fixture() {
        use super::build_live_translate_setup_value;

        #[derive(Deserialize)]
        struct Doc {
            input: Input,
            expect: serde_json::Value,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            model: String,
            target_language: String,
        }

        let doc: Doc = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/gemini-s2s-setup/live-translate.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");

        let actual = build_live_translate_setup_value(&doc.input.model, &doc.input.target_language);
        assert_eq!(actual, doc.expect);
    }
}
