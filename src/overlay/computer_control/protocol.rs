//! Wire protocol for the Computer Control Gemini Live session: the setup
//! payload, tool (function) declarations, realtime-input message builders, and a
//! decoder that turns raw server frames into typed [`ServerEvent`]s.
//!
//! This is the foundational layer the probe (and later the full runtime) build
//! on. Unlike the translate-only parsers (`parse_update`/`parse_s2s_update`),
//! this handles ALL message types — `toolCall`/`toolCallCancellation`/`goAway`/
//! `sessionResumptionUpdate`/`usageMetadata` — and iterates every audio part.

use serde_json::{Value, json};

use crate::api::gemini_live::server_frame::parse_server_frame;
use crate::api::realtime_audio::websocket::pcm_bytes_to_i16;

/// The Live model that backs Computer Control (catalog id `gemini-live-vision-3.1`).
pub const MODEL: &str = crate::model_config::GEMINI_LIVE_API_MODEL_3_1;

/// Preserve the endpoint's native reasoning level unless the operator explicitly
/// overrides it. Thought parts stay enabled so intent never has to be inferred
/// from narration.
pub(crate) fn thinking_config() -> Value {
    thinking_config_for(
        std::env::var("CC_THINK")
            .ok()
            .filter(|value| !value.trim().is_empty()),
    )
}

fn thinking_config_for(level: Option<String>) -> Value {
    let mut config = json!({"includeThoughts": true});
    if let Some(level) = level {
        config["thinkingLevel"] = json!(level);
    }
    config
}

/// Function declarations exposed to the model. Mirrors the Computer-Use action
/// shape but executed natively on Windows. The probe declares a minimal set to
/// verify tool-call emission; the full executor extends this.
pub fn tool_declarations() -> Value {
    json!([{ "functionDeclarations": [
        {
            "name": "click",
            "description": "Click at (x, y). Coordinates are NORMALIZED to a 0-1000 grid over the screenshot: x=0 is the left edge, x=1000 the right edge, y=0 the top edge, y=1000 the bottom edge.",
            "parameters": { "type": "object", "properties": {
                "x": {"type": "integer", "description": "X normalized 0-1000"},
                "y": {"type": "integer", "description": "Y normalized 0-1000"},
                "button": {"type": "string", "enum": ["left", "right", "middle"], "description": "Mouse button (default left)"}
            }, "required": ["x", "y"] }
        },
        {
            "name": "double_click",
            "description": "Double-click at (x, y), normalized to a 0-1000 grid over the screenshot.",
            "parameters": { "type": "object", "properties": {
                "x": {"type": "integer", "description": "X normalized 0-1000"},
                "y": {"type": "integer", "description": "Y normalized 0-1000"}
            }, "required": ["x", "y"] }
        },
        {
            "name": "drag",
            "description": "Press the left button at (x, y) and release at (dest_x, dest_y). All coordinates normalized to a 0-1000 grid over the screenshot.",
            "parameters": { "type": "object", "properties": {
                "x": {"type": "integer", "description": "X normalized 0-1000"},
                "y": {"type": "integer", "description": "Y normalized 0-1000"},
                "dest_x": {"type": "integer", "description": "Destination X normalized 0-1000"},
                "dest_y": {"type": "integer", "description": "Destination Y normalized 0-1000"}
            }, "required": ["x", "y", "dest_x", "dest_y"] }
        },
        {
            "name": "scroll",
            "description": "Scroll at (x, y) (normalized 0-1000) in the given direction by `magnitude` wheel notches.",
            "parameters": { "type": "object", "properties": {
                "x": {"type": "integer", "description": "X normalized 0-1000"},
                "y": {"type": "integer", "description": "Y normalized 0-1000"},
                "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
                "magnitude": {"type": "number", "description": "Wheel notches (default 3)"}
            }, "required": ["x", "y", "direction"] }
        },
        {
            "name": "type_text",
            "description": "Type the given text at the current keyboard focus.",
            "parameters": { "type": "object", "properties": {
                "text": {"type": "string"}
            }, "required": ["text"] }
        },
        {
            "name": "key_combination",
            "description": "Press a keyboard shortcut, e.g. \"Control+C\", \"Alt+Tab\", \"Win+D\", \"Enter\".",
            "parameters": { "type": "object", "properties": {
                "keys": {"type": "string"}
            }, "required": ["keys"] }
        },
        {
            "name": "done",
            "description": "Call when the requested task is complete or cannot proceed. Provide a short summary.",
            "parameters": { "type": "object", "properties": {
                "summary": {"type": "string"}
            }, "required": ["summary"] }
        }
    ]}])
}

/// Build the BidiGenerateContent `setup` payload for the probe (AUDIO output).
pub fn build_setup(system_instruction: &str) -> Value {
    crate::api::gemini_live::setup::LiveSetupBuilder::new(MODEL)
        // HIGH is the OCR knob — required to read small on-screen text.
        .media_resolution(crate::api::gemini_live::setup::MediaResolution::High)
        .voice("Aoede")
        .thinking_override(thinking_config())
        .system_instruction(system_instruction)
        .transcription(crate::api::gemini_live::setup::TranscriptionMode::Both)
        .context_window_compression()
        .setup_field("tools", tool_declarations())
        .setup_field("sessionResumption", json!({}))
        .build()
}

/// One ambiguity invariant not tied to grammar, keywords, language, or app.
pub fn session_rules() -> &'static str {
    "Interpret communicative intent, not grammatical form. If the requested outcome is too uncertain to choose an effect safely, ask one concise clarification and do not act."
}

/// `realtimeInput` carrying one JPEG screen frame (base64).
pub fn realtime_video_jpeg_b64(b64_jpeg: &str) -> Value {
    json!({"realtimeInput": {"video": {"data": b64_jpeg, "mimeType": "image/jpeg"}}})
}

/// `realtimeInput` carrying a text turn.
pub fn realtime_text(text: &str) -> Value {
    json!({"realtimeInput": {"text": text}})
}

/// `toolResponse` answering one function call (match strictly by `id`).
pub fn tool_response(id: &str, name: &str, response: Value) -> Value {
    json!({"toolResponse": {"functionResponses": [{"id": id, "name": name, "response": response}]}})
}

/// One typed thing a server frame can carry. A single frame may yield several.
#[derive(Debug, Clone)]
pub enum ServerEvent {
    SetupComplete,
    /// Decoded model output audio (PCM16 mono 24 kHz).
    Audio(Vec<i16>),
    ModelText(String),
    /// The model's SILENT thinking (includeThoughts) - routed to intent, never spoken/shown.
    Thought(String),
    InputTranscript(String),
    OutputTranscript(String),
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    ToolCancellation(Vec<String>),
    TurnComplete,
    Interrupted,
    GoAway {
        time_left: String,
    },
    SessionResumption {
        handle: Option<String>,
        resumable: bool,
    },
    Usage(Value),
    Other(String),
}

/// Decode one raw server text frame into the events it carries. A single frame
/// may yield several events (e.g. an audio part + a transcript + turnComplete).
pub fn parse_server_message(raw: &str) -> Vec<ServerEvent> {
    let Ok(frame) = parse_server_frame(raw) else {
        return vec![ServerEvent::Other(truncate(raw))];
    };
    let mut out = Vec::new();

    if frame.setup_complete {
        out.push(ServerEvent::SetupComplete);
    }
    // A coalesced user turn owns all model output in the same transport frame.
    // Establish interruption and turn identity before routing its typed output.
    if frame.interrupted {
        out.push(ServerEvent::Interrupted);
    }
    if let Some(text) = frame.input_transcript {
        out.push(ServerEvent::InputTranscript(text));
    }
    for bytes in frame.audio_chunks {
        out.push(ServerEvent::Audio(pcm_bytes_to_i16(&bytes)));
    }
    for part in frame.text_parts {
        if part.thought {
            out.push(ServerEvent::Thought(part.text));
        } else {
            out.push(ServerEvent::ModelText(part.text));
        }
    }
    if let Some(text) = frame.output_transcript {
        out.push(ServerEvent::OutputTranscript(text));
    }
    for call in frame.tool_calls {
        out.push(ServerEvent::ToolCall {
            id: call.id,
            name: call.name,
            args: call.args,
        });
    }
    if let Some(ids) = frame.tool_cancellation_ids {
        out.push(ServerEvent::ToolCancellation(ids));
    }
    // A function call belongs to the generation that produced it. Dispatch it
    // before closing that generation, even if the server coalesces both flags
    // into one wire frame.
    if frame.turn_complete {
        out.push(ServerEvent::TurnComplete);
    }
    if let Some(go_away) = frame.go_away {
        out.push(ServerEvent::GoAway {
            time_left: go_away.time_left,
        });
    }
    if let Some(resumption) = frame.session_resumption {
        out.push(ServerEvent::SessionResumption {
            handle: resumption.handle,
            resumable: resumption.resumable,
        });
    }
    if let Some(usage) = frame.usage_metadata {
        out.push(ServerEvent::Usage(usage));
    }
    if let Some(error) = frame.error {
        out.push(ServerEvent::Other(error));
    }
    // Only surface as "Other" if NO known top-level key was present — a known
    // frame that simply carried nothing we model (e.g. `generationComplete`-only
    // serverContent) is not noise.
    if out.is_empty() && !frame.recognized {
        out.push(ServerEvent::Other(truncate(raw)));
    }
    out
}

fn truncate(s: &str) -> String {
    let clipped: String = s.chars().take(240).collect();
    if clipped.len() < s.len() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_uses_endpoint_thinking_default_and_high_media_resolution() {
        let s = build_setup("hi");
        assert_eq!(s["setup"]["model"], "models/gemini-3.1-flash-live-preview");
        let gc = &s["setup"]["generationConfig"];
        assert_eq!(gc["mediaResolution"], "MEDIA_RESOLUTION_HIGH");
        assert_eq!(gc["maxOutputTokens"], 65536);
        assert_eq!(gc["thinkingConfig"]["includeThoughts"], true);
        assert!(thinking_config_for(None).get("thinkingLevel").is_none());
        // The 3.1 trap: must NOT carry the legacy budget knob alongside the level.
        assert!(gc["thinkingConfig"].get("thinkingBudget").is_none());
        assert!(s["setup"]["tools"].is_array());
    }

    #[test]
    fn parses_tool_call() {
        let raw =
            r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"click","args":{"x":10,"y":20}}]}}"#;
        let evs = parse_server_message(raw);
        match &evs[0] {
            ServerEvent::ToolCall { id, name, args } => {
                assert_eq!(id, "c1");
                assert_eq!(name, "click");
                assert_eq!(args["x"], 10);
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn server_content_yields_audio_transcript_and_turn() {
        let raw = r#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AAAA"}}]},"outputTranscription":{"text":"ok"},"turnComplete":true}}"#;
        let evs = parse_server_message(raw);
        assert!(
            evs.iter()
                .any(|e| matches!(e, ServerEvent::Audio(pcm) if !pcm.is_empty()))
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, ServerEvent::OutputTranscript(t) if t == "ok"))
        );
        assert!(evs.iter().any(|e| matches!(e, ServerEvent::TurnComplete)));
    }

    #[test]
    fn coalesced_tool_call_precedes_its_turn_boundary() {
        let raw = r#"{"serverContent":{"turnComplete":true},"toolCall":{"functionCalls":[{"id":"d1","name":"done","args":{"summary":"complete"}}]}}"#;
        let evs = parse_server_message(raw);
        let call = evs
            .iter()
            .position(|event| matches!(event, ServerEvent::ToolCall { .. }))
            .unwrap();
        let boundary = evs
            .iter()
            .position(|event| matches!(event, ServerEvent::TurnComplete))
            .unwrap();
        assert!(call < boundary);
    }

    #[test]
    fn coalesced_user_turn_precedes_model_output() {
        let raw = r#"{"serverContent":{"inputTranscription":{"text":"new goal"},"outputTranscription":{"text":"answer"}}}"#;
        let events = parse_server_message(raw);
        let input = events
            .iter()
            .position(|event| matches!(event, ServerEvent::InputTranscript(_)))
            .unwrap();
        let output = events
            .iter()
            .position(|event| matches!(event, ServerEvent::OutputTranscript(_)))
            .unwrap();
        assert!(input < output);
    }
}
