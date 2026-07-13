use anyhow::Result;
use native_tls::TlsStream;
use std::net::TcpStream;
use tungstenite::WebSocket;

/// Create TLS WebSocket connection to Gemini Live API for TTS
pub fn connect_tts_websocket(api_key: &str) -> Result<WebSocket<TlsStream<TcpStream>>> {
    crate::api::gemini_live::transport::connect_websocket(api_key)
}

/// Build the TTS setup envelope for use by setup-gated Live sessions.
pub fn build_tts_setup(
    model: &str,
    voice_name: &str,
    speed: &str,
    custom_instructions: Option<&str>,
) -> serde_json::Value {
    // System instruction based on speed
    let mut system_text = "You are a text-to-speech reader. Your ONLY job is to read the user's text out loud, exactly as written, word for word. Do NOT respond conversationally. Do NOT add commentary. Do NOT ask questions. ".to_string();

    match speed {
        "Slow" => system_text.push_str("Speak slowly, clearly, and with deliberate pacing. "),
        "Fast" => system_text.push_str("Speak quickly, efficiently, and with a brisk pace. "),
        _ => system_text.push_str("Simply read the provided text aloud naturally and clearly. "),
    }

    // Append custom tone/style instructions if provided
    if let Some(instructions) = custom_instructions
        && !instructions.trim().is_empty()
    {
        system_text.push_str(" Additional instructions: ");
        system_text.push_str(instructions.trim());
        system_text.push(' ');
    }

    system_text.push_str("Start reading immediately.");

    crate::api::gemini_live::setup::LiveSetupBuilder::new(model)
        .media_resolution(crate::api::gemini_live::setup::MediaResolution::Low)
        .voice(voice_name)
        .system_instruction(&system_text)
        .build()
}

/// Build one TTS request payload for a ready Live session.
pub fn build_tts_text(text: &str) -> serde_json::Value {
    // Format with explicit instruction to read verbatim
    let prompt = format!("[READ ALOUD VERBATIM - START NOW]\n\n{}", text);

    serde_json::json!({
        "realtimeInput": {
            "text": prompt
        }
    })
}
