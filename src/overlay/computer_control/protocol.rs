//! Wire protocol for the Computer Control Gemini Live session: the setup
//! payload, tool (function) declarations, realtime-input message builders, and a
//! decoder that turns raw server frames into typed [`ServerEvent`]s.
//!
//! This is the foundational layer the probe (and later the full runtime) build
//! on. Unlike the translate-only parsers (`parse_update`/`parse_s2s_update`),
//! this handles ALL message types — `toolCall`/`toolCallCancellation`/`goAway`/
//! `sessionResumptionUpdate`/`usageMetadata` — and iterates every audio part.

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

use crate::api::realtime_audio::websocket::pcm_bytes_to_i16;

/// The Live model that backs Computer Control (catalog id `gemini-live-vision-3.1`).
pub const MODEL: &str = "gemini-3.1-flash-live-preview";

/// Reasoning budget for 3.1: `minimal|low|medium|high`. Overridable via `CC_THINK`
/// for debugging (default `low`).
fn thinking_level() -> String {
    std::env::var("CC_THINK")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "low".to_string())
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
    json!({ "setup": {
        "model": format!("models/{MODEL}"),
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": "Aoede"}}},
            // HIGH is the OCR knob — required to read small on-screen text.
            "mediaResolution": "MEDIA_RESOLUTION_HIGH",
            // 3.1 uses thinkingLevel (NOT the legacy thinkingBudget); "low"
            // balances tool-call reasoning against latency.
            // includeThoughts surfaces the model's SILENT reasoning as text parts (thought:true),
            // which we capture as per-turn intent — never spoken, never shown. (If the preview model
            // rejects this field with INVALID_ARGUMENT, drop includeThoughts.)
            "thinkingConfig": {"thinkingLevel": thinking_level(), "includeThoughts": true}
        },
        "systemInstruction": {"parts": [{"text": system_instruction}]},
        "tools": tool_declarations(),
        "inputAudioTranscription": {},
        "outputAudioTranscription": {},
        "sessionResumption": {},
        "contextWindowCompression": {"slidingWindow": {}}
    }})
}

/// Behavioural overlay appended to the LIVE system prompt — the JUDGMENT layer SYS underweights (it pushes
/// autonomy in 4 places with only one weak "ask" escape): when to act vs. ASK (questions, the user's own
/// data / account choices, unclear requests), and never blind-clicking destructive controls that can wipe
/// the user's work. Balanced — autonomous by default, asks only for the three named cases. NO language
/// anchoring; works in any language; the user never sees this.
pub fn session_rules() -> &'static str {
    "INTENT FIRST: at the start of a turn, silently settle in ONE line what the user wants from what you \
HEARD (e.g. 'play this video', 'go back') - never from a word on screen - then pursue THAT. \
JUDGMENT (act vs. ask): DEFAULT TO ACTING and keep going - carry out the task's mechanical steps \
back-to-back; do NOT pause to ask 'shall we / do you want' between them, and do NOT narrate every step \
(it is slow and the user finds it tiring). BUT if one step or your thinking is taking a WHILE (writing code, a \
long search or read), say ONE short line about what you're doing so the user knows you're still on it - many \
seconds of silence reads as 'frozen'. Only STOP to ask when: (a) the user asks you a question or for \
advice ('what should I', 'do you think', 'what would') - answer in WORDS, do not act on it; (b) a step needs \
the user's OWN data or choice you were NOT given (a username, which account or email, a password, payment \
details) - NEVER invent it from what is on screen; (c) the request makes no sense for what's on screen - it \
sounds garbled, or you catch yourself GUESSING what a word means or wanting to look UP its meaning: that is a \
MIS-HEARING, not a task. Say what you heard in ONE short line and ask them to repeat it - do NOT act on the \
guess, and NEVER both act on it AND research what it means (doing both is proof you did not understand). Also \
do ONE thing per command: if you have done it, STOP - do not wander into nearby actions you were not asked for; or (d) you \
are about to do something CONSEQUENTIAL or IRREVERSIBLE - send or post a message / email / comment, publish \
content, pay / buy / transfer money, create or delete an account, or submit personal or financial data - in \
that case confirm THAT exact action with the user first, and only then do it (for the act tool, pass \
confirm:true only after they agree). If something unexpected or destructive happens, STOP and tell the user - \
do NOT silently undo or redo it."
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
    ToolCall { id: String, name: String, args: Value },
    ToolCancellation(Vec<String>),
    TurnComplete,
    Interrupted,
    GoAway { time_left: String },
    SessionResumption { handle: Option<String>, resumable: bool },
    Usage(Value),
    Other(String),
}

/// Decode one raw server text frame into the events it carries. A single frame
/// may yield several events (e.g. an audio part + a transcript + turnComplete).
pub fn parse_server_message(raw: &str) -> Vec<ServerEvent> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return vec![ServerEvent::Other(truncate(raw))];
    };
    let mut out = Vec::new();

    if v.get("setupComplete").is_some() {
        out.push(ServerEvent::SetupComplete);
    }
    if let Some(sc) = v.get("serverContent") {
        if let Some(parts) = sc.pointer("/modelTurn/parts").and_then(|p| p.as_array()) {
            for part in parts {
                if let Some(data) = part.pointer("/inlineData/data").and_then(Value::as_str)
                    && let Ok(bytes) = general_purpose::STANDARD.decode(data)
                {
                    out.push(ServerEvent::Audio(pcm_bytes_to_i16(&bytes)));
                }
                if let Some(t) = part.get("text").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    // A thought part (includeThoughts) is the model's SILENT reasoning - route it
                    // to intent, not to the spoken/ignored ModelText path.
                    if part.get("thought").and_then(Value::as_bool) == Some(true) {
                        out.push(ServerEvent::Thought(t.to_string()));
                    } else {
                        out.push(ServerEvent::ModelText(t.to_string()));
                    }
                }
            }
        }
        if let Some(t) = sc.pointer("/inputTranscription/text").and_then(|t| t.as_str()) {
            out.push(ServerEvent::InputTranscript(t.to_string()));
        }
        if let Some(t) = sc.pointer("/outputTranscription/text").and_then(|t| t.as_str()) {
            out.push(ServerEvent::OutputTranscript(t.to_string()));
        }
        if sc.get("interrupted").and_then(Value::as_bool) == Some(true) {
            out.push(ServerEvent::Interrupted);
        }
        if sc.get("turnComplete").and_then(Value::as_bool) == Some(true) {
            out.push(ServerEvent::TurnComplete);
        }
    }
    if let Some(calls) = v.pointer("/toolCall/functionCalls").and_then(|c| c.as_array()) {
        for c in calls {
            out.push(ServerEvent::ToolCall {
                id: c.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                name: c.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                args: c.get("args").cloned().unwrap_or(Value::Null),
            });
        }
    }
    if let Some(ids) = v.pointer("/toolCallCancellation/ids").and_then(|i| i.as_array()) {
        out.push(ServerEvent::ToolCancellation(
            ids.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        ));
    }
    if let Some(g) = v.get("goAway") {
        out.push(ServerEvent::GoAway {
            time_left: g.get("timeLeft").and_then(Value::as_str).unwrap_or("").to_string(),
        });
    }
    if let Some(s) = v.get("sessionResumptionUpdate") {
        out.push(ServerEvent::SessionResumption {
            handle: s.get("newHandle").and_then(Value::as_str).map(str::to_string),
            resumable: s.get("resumable").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    if let Some(u) = v.get("usageMetadata") {
        out.push(ServerEvent::Usage(u.clone()));
    }
    // Only surface as "Other" if NO known top-level key was present — a known
    // frame that simply carried nothing we model (e.g. `generationComplete`-only
    // serverContent) is not noise.
    let recognized = ["setupComplete", "serverContent", "toolCall", "toolCallCancellation",
        "goAway", "sessionResumptionUpdate", "usageMetadata"]
        .iter()
        .any(|k| v.get(k).is_some());
    if out.is_empty() && !recognized {
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
    fn setup_uses_thinking_level_and_high_media_resolution() {
        let s = build_setup("hi");
        assert_eq!(s["setup"]["model"], "models/gemini-3.1-flash-live-preview");
        let gc = &s["setup"]["generationConfig"];
        assert_eq!(gc["mediaResolution"], "MEDIA_RESOLUTION_HIGH");
        assert_eq!(gc["thinkingConfig"]["thinkingLevel"], "low");
        // The 3.1 trap: must NOT carry the legacy budget knob alongside the level.
        assert!(gc["thinkingConfig"].get("thinkingBudget").is_none());
        assert!(s["setup"]["tools"].is_array());
    }

    #[test]
    fn parses_tool_call() {
        let raw = r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"click","args":{"x":10,"y":20}}]}}"#;
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
        assert!(evs.iter().any(|e| matches!(e, ServerEvent::Audio(pcm) if !pcm.is_empty())));
        assert!(evs.iter().any(|e| matches!(e, ServerEvent::OutputTranscript(t) if t == "ok")));
        assert!(evs.iter().any(|e| matches!(e, ServerEvent::TurnComplete)));
    }
}
