//! Wire protocol for the Computer Control Gemini Live session: the setup
//! payload, tool (function) declarations, realtime-input message builders, and a
//! decoder that turns raw server frames into typed [`ServerEvent`]s.
//!
//! This is the foundational layer the probe (and later the full runtime) build
//! on. Unlike the translate-only parsers (`parse_update`/`parse_s2s_update`),
//! this handles ALL message types — `toolCall`/`toolCallCancellation`/`goAway`/
//! `sessionResumptionUpdate`/`usageMetadata` — and iterates every audio part.

use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use serde_json::{Value, json};
use sgt_computer_control_protocol::{MAX_PROVIDER_FRAME_BYTES, ProviderEvent};

#[cfg(test)]
use crate::api::gemini_live::server_frame::parse_server_frame;
#[cfg(test)]
use crate::api::realtime_audio::websocket::pcm_bytes_to_i16;

use super::engine;

const MAX_PROVIDER_EVENTS: usize = 1_024;

/// The Live model that backs Computer Control (catalog id `google-gemini-3-1-live-vision`).
pub const MODEL: &str = crate::model_config::GEMINI_LIVE_API_MODEL_3_1;

/// Use deliberate low thinking for control accuracy while keeping latency bounded.
/// Thought parts stay enabled so intent never has to be inferred from narration.
pub(crate) fn thinking_config() -> Value {
    thinking_config_for(
        std::env::var("CC_THINK")
            .ok()
            .filter(|value| !value.trim().is_empty()),
    )
}

fn thinking_config_for(level: Option<String>) -> Value {
    json!({
        "includeThoughts": true,
        "thinkingLevel": level.unwrap_or_else(|| "LOW".to_string())
    })
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
    /// The model has finished producing this generation. With realtime audio,
    /// `TurnComplete` may intentionally follow only after expected playback.
    GenerationComplete,
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
pub fn parse_server_message(raw: &str) -> Result<Vec<ServerEvent>> {
    map_provider_events(engine::parse_provider_frame(raw)?)
}

fn map_provider_events(events: Vec<ProviderEvent>) -> Result<Vec<ServerEvent>> {
    if events.len() > MAX_PROVIDER_EVENTS {
        bail!("Computer Control engine returned too many provider events");
    }
    events
        .into_iter()
        .map(|event| {
            Ok(match event {
                ProviderEvent::SetupComplete => ServerEvent::SetupComplete,
                ProviderEvent::AudioPcm16 { data_base64 } => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data_base64)
                        .map_err(|_| anyhow!("Computer Control engine returned invalid audio"))?;
                    if bytes.len() > MAX_PROVIDER_FRAME_BYTES || bytes.len() % 2 != 0 {
                        bail!("Computer Control engine returned invalid PCM16 audio");
                    }
                    ServerEvent::Audio(
                        bytes
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                            .collect(),
                    )
                }
                ProviderEvent::ModelText { text } => {
                    ServerEvent::ModelText(valid_text(text, MAX_PROVIDER_FRAME_BYTES, "text")?)
                }
                ProviderEvent::Thought { text } => {
                    ServerEvent::Thought(valid_text(text, MAX_PROVIDER_FRAME_BYTES, "thought")?)
                }
                ProviderEvent::InputTranscript { text } => ServerEvent::InputTranscript(
                    valid_text(text, MAX_PROVIDER_FRAME_BYTES, "input transcript")?,
                ),
                ProviderEvent::OutputTranscript { text } => ServerEvent::OutputTranscript(
                    valid_text(text, MAX_PROVIDER_FRAME_BYTES, "output transcript")?,
                ),
                ProviderEvent::ToolCall { id, name, args } => {
                    if !args.is_object() {
                        bail!("Computer Control engine returned non-object tool arguments");
                    }
                    ServerEvent::ToolCall {
                        id: valid_text(id, 256, "tool call id")?,
                        name: valid_tool_name(name)?,
                        args,
                    }
                }
                ProviderEvent::ToolCancellation { ids } => {
                    if ids.len() > 256 {
                        bail!("Computer Control engine returned too many cancellations");
                    }
                    ServerEvent::ToolCancellation(
                        ids.into_iter()
                            .map(|id| valid_text(id, 256, "tool cancellation id"))
                            .collect::<Result<Vec<_>>>()?,
                    )
                }
                ProviderEvent::GenerationComplete => ServerEvent::GenerationComplete,
                ProviderEvent::TurnComplete => ServerEvent::TurnComplete,
                ProviderEvent::Interrupted => ServerEvent::Interrupted,
                ProviderEvent::GoAway { time_left } => ServerEvent::GoAway {
                    time_left: valid_text(time_left, 256, "go-away duration")?,
                },
                ProviderEvent::SessionResumption { handle, resumable } => {
                    ServerEvent::SessionResumption {
                        handle: handle
                            .map(|value| valid_text(value, 8 * 1024, "resumption handle"))
                            .transpose()?,
                        resumable,
                    }
                }
                ProviderEvent::Usage { metadata } => {
                    if serde_json::to_vec(&metadata)?.len() > 64 * 1024 {
                        bail!("Computer Control engine returned oversized usage metadata");
                    }
                    ServerEvent::Usage(metadata)
                }
                ProviderEvent::Other { summary } => {
                    ServerEvent::Other(valid_text(summary, 1_024, "provider summary")?)
                }
            })
        })
        .collect()
}

fn valid_text(value: String, maximum: usize, label: &str) -> Result<String> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        bail!("Computer Control engine returned invalid {label}");
    }
    Ok(value)
}

fn valid_tool_name(value: String) -> Result<String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        bail!("Computer Control engine returned an invalid tool name");
    }
    Ok(value)
}

#[cfg(test)]
fn parse_server_message_canonical(raw: &str) -> Vec<ServerEvent> {
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
    if frame.generation_complete {
        out.push(ServerEvent::GenerationComplete);
    }
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

#[cfg(test)]
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
    fn normalize(event: ServerEvent) -> Value {
        match event {
            ServerEvent::SetupComplete => json!({"type": "setup-complete"}),
            ServerEvent::Audio(samples) => {
                let bytes = samples
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>();
                json!({
                    "type": "audio-pcm16",
                    "dataBase64": base64::engine::general_purpose::STANDARD.encode(bytes)
                })
            }
            ServerEvent::ModelText(text) => json!({"type": "model-text", "text": text}),
            ServerEvent::Thought(text) => json!({"type": "thought", "text": text}),
            ServerEvent::InputTranscript(text) => {
                json!({"type": "input-transcript", "text": text})
            }
            ServerEvent::OutputTranscript(text) => {
                json!({"type": "output-transcript", "text": text})
            }
            ServerEvent::ToolCall { id, name, args } => {
                json!({"type": "tool-call", "id": id, "name": name, "args": args})
            }
            ServerEvent::ToolCancellation(ids) => {
                json!({"type": "tool-cancellation", "ids": ids})
            }
            ServerEvent::GenerationComplete => json!({"type": "generation-complete"}),
            ServerEvent::TurnComplete => json!({"type": "turn-complete"}),
            ServerEvent::Interrupted => json!({"type": "interrupted"}),
            ServerEvent::GoAway { time_left } => {
                json!({"type": "go-away", "timeLeft": time_left})
            }
            ServerEvent::SessionResumption { handle, resumable } => json!({
                "type": "session-resumption",
                "handle": handle,
                "resumable": resumable
            }),
            ServerEvent::Usage(metadata) => json!({"type": "usage", "metadata": metadata}),
            ServerEvent::Other(summary) => json!({"type": "other", "summary": summary}),
        }
    }

    #[test]
    fn canonical_parser_matches_shared_provider_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/computer-control-engine/protocol-v1.json"
        )))
        .unwrap();
        for case in fixture["providerFrames"].as_array().unwrap() {
            if case["strictOnly"].as_bool() == Some(true) {
                continue;
            }
            let actual = parse_server_message_canonical(&case["frame"].to_string())
                .into_iter()
                .map(normalize)
                .collect::<Vec<_>>();
            assert_eq!(
                Value::Array(actual),
                case["expected"],
                "case {}",
                case["id"]
            );
        }
    }

    #[test]
    fn setup_uses_low_thinking_and_high_media_resolution() {
        let s = build_setup("hi");
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/phone-control/model-chain.json"
        )))
        .expect("Phone Control model-chain fixture parses");
        let live = &fixture["live_session"];
        assert_eq!(
            s["setup"]["model"],
            format!("models/{}", live["api_model"].as_str().unwrap())
        );
        let gc = &s["setup"]["generationConfig"];
        assert_eq!(gc["mediaResolution"], "MEDIA_RESOLUTION_HIGH");
        assert_eq!(gc["maxOutputTokens"], 65536);
        assert_eq!(gc["thinkingConfig"], live["thinking_config"]);
        // The 3.1 trap: must NOT carry the legacy budget knob alongside the level.
        assert!(gc["thinkingConfig"].get("thinkingBudget").is_none());
        assert!(s["setup"]["tools"].is_array());
    }

    #[test]
    fn parses_tool_call() {
        let raw =
            r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"click","args":{"x":10,"y":20}}]}}"#;
        let evs = parse_server_message_canonical(raw);
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
        let raw = r#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AAAA"}}]},"outputTranscription":{"text":"ok"},"generationComplete":true,"turnComplete":true}}"#;
        let evs = parse_server_message_canonical(raw);
        assert!(
            evs.iter()
                .any(|e| matches!(e, ServerEvent::Audio(pcm) if !pcm.is_empty()))
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, ServerEvent::OutputTranscript(t) if t == "ok"))
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, ServerEvent::GenerationComplete))
        );
        assert!(evs.iter().any(|e| matches!(e, ServerEvent::TurnComplete)));
    }

    #[test]
    fn generation_and_turn_completion_remain_distinct_ordered_signals() {
        let events = parse_server_message_canonical(
            r#"{"serverContent":{"generationComplete":true,"turnComplete":true}}"#,
        );
        assert!(matches!(
            events.as_slice(),
            [ServerEvent::GenerationComplete, ServerEvent::TurnComplete]
        ));

        let generation_only = parse_server_message_canonical(
            r#"{"serverContent":{"generationComplete":true,"turnComplete":false}}"#,
        );
        assert!(matches!(
            generation_only.as_slice(),
            [ServerEvent::GenerationComplete]
        ));
    }

    #[test]
    fn coalesced_tool_call_precedes_its_turn_boundary() {
        let raw = r#"{"serverContent":{"turnComplete":true},"toolCall":{"functionCalls":[{"id":"d1","name":"done","args":{"summary":"complete"}}]}}"#;
        let evs = parse_server_message_canonical(raw);
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
        let events = parse_server_message_canonical(raw);
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

    #[test]
    fn host_rejects_malformed_effect_bearing_engine_output() {
        assert!(
            map_provider_events(vec![ProviderEvent::ToolCall {
                id: "call-1".to_string(),
                name: "click".to_string(),
                args: json!([]),
            }])
            .is_err()
        );
        assert!(
            map_provider_events(vec![ProviderEvent::ToolCall {
                id: "call-1".to_string(),
                name: "invalid name".to_string(),
                args: json!({}),
            }])
            .is_err()
        );
        assert!(
            map_provider_events(vec![ProviderEvent::AudioPcm16 {
                data_base64: base64::engine::general_purpose::STANDARD.encode([1_u8]),
            }])
            .is_err()
        );
    }
}
