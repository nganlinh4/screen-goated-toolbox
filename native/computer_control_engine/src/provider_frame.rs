use base64::{Engine as _, engine::general_purpose};
use serde_json::Value;
use sgt_computer_control_protocol::ProviderEvent;

const MAX_OTHER_CHARS: usize = 240;
const MAX_TOOL_ID_BYTES: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 128;

pub(super) fn parse(raw: &str) -> Vec<ProviderEvent> {
    let Ok(root) = serde_json::from_str::<Value>(raw) else {
        return vec![other(raw)];
    };
    let Some(root) = root.as_object() else {
        return vec![other(raw)];
    };
    let recognized = [
        "setupComplete",
        "serverContent",
        "error",
        "toolCall",
        "toolCallCancellation",
        "goAway",
        "sessionResumptionUpdate",
        "usageMetadata",
    ]
    .iter()
    .any(|key| root.contains_key(*key));
    let generation_complete = root
        .get("serverContent")
        .and_then(|content| content.get("generationComplete"))
        .and_then(Value::as_bool)
        == Some(true);
    let turn_complete = root
        .get("serverContent")
        .and_then(|content| content.get("turnComplete"))
        .and_then(Value::as_bool)
        == Some(true);
    let mut events = Vec::new();
    if root.contains_key("setupComplete") {
        events.push(ProviderEvent::SetupComplete);
    }

    if let Some(content) = root.get("serverContent") {
        if content.get("interrupted").and_then(Value::as_bool) == Some(true) {
            events.push(ProviderEvent::Interrupted);
        }
        if let Some(text) = transcription(content, "inputTranscription") {
            events.push(ProviderEvent::InputTranscript { text });
        }
        if let Some(parts) = content
            .pointer("/modelTurn/parts")
            .and_then(Value::as_array)
        {
            for part in parts {
                if let Some(data) = valid_audio(part) {
                    events.push(ProviderEvent::AudioPcm16 { data_base64: data });
                }
            }
            for part in parts {
                let Some(text) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_string)
                else {
                    continue;
                };
                if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    events.push(ProviderEvent::Thought { text });
                } else {
                    events.push(ProviderEvent::ModelText { text });
                }
            }
        }
        if let Some(text) = transcription(content, "outputTranscription") {
            events.push(ProviderEvent::OutputTranscript { text });
        }
    }

    if let Some(calls) = root
        .get("toolCall")
        .and_then(|call| call.get("functionCalls"))
        .and_then(Value::as_array)
    {
        for call in calls {
            match tool_call(call) {
                Some(event) => events.push(event),
                None => events.push(ProviderEvent::Other {
                    summary: "malformed provider tool call rejected".to_string(),
                }),
            }
        }
    }
    if let Some(cancellation) = root.get("toolCallCancellation") {
        let ids = cancellation
            .get("ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= MAX_TOOL_ID_BYTES)
            .map(str::to_string)
            .collect();
        events.push(ProviderEvent::ToolCancellation { ids });
    }

    if generation_complete {
        events.push(ProviderEvent::GenerationComplete);
    }
    if turn_complete {
        events.push(ProviderEvent::TurnComplete);
    }
    if let Some(go_away) = root.get("goAway") {
        events.push(ProviderEvent::GoAway {
            time_left: go_away
                .get("timeLeft")
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(128)
                .collect(),
        });
    }
    if let Some(resumption) = root.get("sessionResumptionUpdate") {
        events.push(ProviderEvent::SessionResumption {
            handle: resumption
                .get("newHandle")
                .and_then(Value::as_str)
                .filter(|handle| handle.len() <= 16 * 1024)
                .map(str::to_string),
            resumable: resumption.get("resumable").and_then(Value::as_bool) == Some(true),
        });
    }
    if let Some(metadata) = root.get("usageMetadata") {
        events.push(ProviderEvent::Usage {
            metadata: metadata.clone(),
        });
    }
    if let Some(error) = root.get("error").and_then(protocol_error) {
        events.push(ProviderEvent::Other { summary: error });
    }
    if events.is_empty() && !recognized {
        events.push(other(raw));
    }
    events
}

fn valid_audio(part: &Value) -> Option<String> {
    let data = part.pointer("/inlineData/data")?.as_str()?;
    if data.trim().is_empty() {
        return None;
    }
    let bytes = general_purpose::STANDARD.decode(data).ok()?;
    (!bytes.is_empty() && bytes.len().is_multiple_of(2)).then(|| data.to_string())
}

fn transcription(content: &Value, field: &str) -> Option<String> {
    content
        .get(field)
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

fn tool_call(call: &Value) -> Option<ProviderEvent> {
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_TOOL_ID_BYTES)?;
    let name = call.get("name").and_then(Value::as_str).filter(|name| {
        !name.is_empty()
            && name.len() <= MAX_TOOL_NAME_BYTES
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    })?;
    let args = call
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !args.is_object() {
        return None;
    }
    Some(ProviderEvent::ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        args,
    })
}

fn protocol_error(error: &Value) -> Option<String> {
    match error {
        Value::Null => None,
        Value::String(message) => (!message.trim().is_empty()).then(|| truncate(message)),
        Value::Object(_) => error
            .get("message")
            .and_then(Value::as_str)
            .filter(|message| !message.trim().is_empty())
            .map(truncate)
            .or_else(|| Some(truncate(&error.to_string()))),
        _ => Some(truncate(&error.to_string())),
    }
}

fn other(raw: &str) -> ProviderEvent {
    ProviderEvent::Other {
        summary: truncate(raw),
    }
}

fn truncate(value: &str) -> String {
    let clipped = value.chars().take(MAX_OTHER_CHARS).collect::<String>();
    if clipped.len() < value.len() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_provider_event_order_and_all_audio_parts() {
        let events = parse(
            r#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AQI="}},{"inlineData":{"data":"AwQ="}},{"text":"thought","thought":true},{"text":"visible"}]},"inputTranscription":{"text":"heard"},"outputTranscription":{"text":"spoken"},"interrupted":true,"generationComplete":true,"turnComplete":true},"toolCall":{"functionCalls":[{"id":"c1","name":"future_tool","args":{"x":1}}]}}"#,
        );
        assert!(matches!(events[0], ProviderEvent::Interrupted));
        assert!(matches!(events[1], ProviderEvent::InputTranscript { .. }));
        assert!(matches!(events[2], ProviderEvent::AudioPcm16 { .. }));
        assert!(matches!(events[3], ProviderEvent::AudioPcm16 { .. }));
        let call = events
            .iter()
            .position(|event| matches!(event, ProviderEvent::ToolCall { .. }))
            .unwrap();
        let boundary = events
            .iter()
            .position(|event| matches!(event, ProviderEvent::GenerationComplete))
            .unwrap();
        assert!(call < boundary);
    }

    #[test]
    fn malformed_tool_calls_fail_closed_without_keyword_routing() {
        let events = parse(
            r#"{"toolCall":{"functionCalls":[{"id":"","name":"run_command","args":[]},{"id":"2","name":"future-tool","args":{}}]}}"#,
        );
        assert!(matches!(events[0], ProviderEvent::Other { .. }));
        assert!(
            matches!(events[1], ProviderEvent::ToolCall { ref name, .. } if name == "future-tool")
        );
    }

    #[test]
    fn protocol_fields_are_structural_not_text_searches() {
        assert_eq!(
            parse(r#"{"note":"setupComplete toolCall turnComplete"}"#),
            vec![ProviderEvent::Other {
                summary: r#"{"note":"setupComplete toolCall turnComplete"}"#.to_string()
            }]
        );
    }

    #[test]
    fn provider_events_match_the_shared_protocol_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/computer-control-engine/protocol-v1.json"
        ))
        .unwrap();
        for case in fixture["providerFrames"].as_array().unwrap() {
            let actual = parse(&case["frame"].to_string())
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

    fn normalize(event: ProviderEvent) -> Value {
        match event {
            ProviderEvent::SetupComplete => json!({"type": "setup-complete"}),
            ProviderEvent::AudioPcm16 { data_base64 } => {
                json!({"type": "audio-pcm16", "dataBase64": data_base64})
            }
            ProviderEvent::ModelText { text } => json!({"type": "model-text", "text": text}),
            ProviderEvent::Thought { text } => json!({"type": "thought", "text": text}),
            ProviderEvent::InputTranscript { text } => {
                json!({"type": "input-transcript", "text": text})
            }
            ProviderEvent::OutputTranscript { text } => {
                json!({"type": "output-transcript", "text": text})
            }
            ProviderEvent::ToolCall { id, name, args } => {
                json!({"type": "tool-call", "id": id, "name": name, "args": args})
            }
            ProviderEvent::ToolCancellation { ids } => {
                json!({"type": "tool-cancellation", "ids": ids})
            }
            ProviderEvent::GenerationComplete => json!({"type": "generation-complete"}),
            ProviderEvent::TurnComplete => json!({"type": "turn-complete"}),
            ProviderEvent::Interrupted => json!({"type": "interrupted"}),
            ProviderEvent::GoAway { time_left } => {
                json!({"type": "go-away", "timeLeft": time_left})
            }
            ProviderEvent::SessionResumption { handle, resumable } => json!({
                "type": "session-resumption",
                "handle": handle,
                "resumable": resumable,
            }),
            ProviderEvent::Usage { metadata } => json!({"type": "usage", "metadata": metadata}),
            ProviderEvent::Other { summary } => json!({"type": "other", "summary": summary}),
        }
    }
}
