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
use sgt_computer_control_protocol::{
    MAX_JSON_BYTES, MAX_PROVIDER_FRAME_BYTES, MAX_PROVIDER_GO_AWAY_CHARS,
    MAX_PROVIDER_RESUMPTION_HANDLE_BYTES, MAX_PROVIDER_TOOL_ID_BYTES, MAX_PROVIDER_TOOL_NAME_BYTES,
    ProviderEvent,
};

use super::engine;

// Even the shortest JSON array items need a value and separator. The framed
// byte budget therefore bounds both arrays without excluding valid wire data.
const MAX_PROVIDER_ARRAY_ITEMS: usize = MAX_JSON_BYTES / 2;

/// The Live model that backs Computer Control (catalog id `google-gemini-3-1-live-vision`).
pub const MODEL: &str = crate::model_config::GEMINI_LIVE_API_MODEL_3_1;

/// Use deliberate low thinking for control accuracy while keeping latency bounded.
/// Thought parts remain private provider progress signals.
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
    decode_provider_events(engine::parse_provider_frame(raw)?)
}

pub(super) fn decode_provider_events(events: Vec<ProviderEvent>) -> Result<Vec<ServerEvent>> {
    map_provider_events(events).map_err(|error| engine::DataRejection::provider_frame(error).into())
}

fn map_provider_events(events: Vec<ProviderEvent>) -> Result<Vec<ServerEvent>> {
    if events.len() > MAX_PROVIDER_ARRAY_ITEMS {
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
                ProviderEvent::ModelText { text } => ServerEvent::ModelText(valid_content_text(
                    text,
                    MAX_PROVIDER_FRAME_BYTES,
                    "text",
                )?),
                ProviderEvent::Thought { text } => ServerEvent::Thought(valid_content_text(
                    text,
                    MAX_PROVIDER_FRAME_BYTES,
                    "thought",
                )?),
                ProviderEvent::InputTranscript { text } => ServerEvent::InputTranscript(
                    valid_content_text(text, MAX_PROVIDER_FRAME_BYTES, "input transcript")?,
                ),
                ProviderEvent::OutputTranscript { text } => ServerEvent::OutputTranscript(
                    valid_content_text(text, MAX_PROVIDER_FRAME_BYTES, "output transcript")?,
                ),
                ProviderEvent::ToolCall { id, name, args } => {
                    if !args.is_object() {
                        bail!("Computer Control engine returned non-object tool arguments");
                    }
                    ServerEvent::ToolCall {
                        id: valid_text(id, MAX_PROVIDER_TOOL_ID_BYTES, "tool call id")?,
                        name: valid_tool_name(name)?,
                        args,
                    }
                }
                ProviderEvent::ToolCancellation { ids } => {
                    if ids.len() > MAX_PROVIDER_ARRAY_ITEMS {
                        bail!("Computer Control engine returned too many cancellations");
                    }
                    ServerEvent::ToolCancellation(
                        ids.into_iter()
                            .map(|id| {
                                valid_text(id, MAX_PROVIDER_TOOL_ID_BYTES, "tool cancellation id")
                            })
                            .collect::<Result<Vec<_>>>()?,
                    )
                }
                ProviderEvent::GenerationComplete => ServerEvent::GenerationComplete,
                ProviderEvent::TurnComplete => ServerEvent::TurnComplete,
                ProviderEvent::Interrupted => ServerEvent::Interrupted,
                ProviderEvent::GoAway { time_left } => {
                    if time_left.chars().count() > MAX_PROVIDER_GO_AWAY_CHARS
                        || time_left.contains('\0')
                    {
                        bail!("Computer Control engine returned invalid go-away duration");
                    }
                    ServerEvent::GoAway { time_left }
                }
                ProviderEvent::SessionResumption { handle, resumable } => {
                    ServerEvent::SessionResumption {
                        handle: handle
                            .map(|value| {
                                valid_optional_text(
                                    value,
                                    MAX_PROVIDER_RESUMPTION_HANDLE_BYTES,
                                    "resumption handle",
                                )
                            })
                            .transpose()?,
                        resumable,
                    }
                }
                ProviderEvent::Usage { metadata } => {
                    if serde_json::to_vec(&metadata)?.len() > MAX_JSON_BYTES {
                        bail!("Computer Control engine returned oversized usage metadata");
                    }
                    ServerEvent::Usage(metadata)
                }
                ProviderEvent::Other { summary } => {
                    ServerEvent::Other(valid_content_text(summary, 1_024, "provider summary")?)
                }
            })
        })
        .collect()
}

fn valid_content_text(value: String, maximum: usize, label: &str) -> Result<String> {
    if value.is_empty() || value.len() > maximum {
        bail!("Computer Control engine returned invalid {label}");
    }
    Ok(value)
}

fn valid_text(value: String, maximum: usize, label: &str) -> Result<String> {
    let value = valid_content_text(value, maximum, label)?;
    if value.contains('\0') {
        bail!("Computer Control engine returned invalid {label}");
    }
    Ok(value)
}

fn valid_optional_text(value: String, maximum: usize, label: &str) -> Result<String> {
    if value.len() > maximum || value.contains('\0') {
        bail!("Computer Control engine returned invalid {label}");
    }
    Ok(value)
}

fn valid_tool_name(value: String) -> Result<String> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_TOOL_NAME_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        bail!("Computer Control engine returned an invalid tool name");
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
