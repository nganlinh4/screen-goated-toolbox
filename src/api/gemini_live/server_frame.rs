//! Structural decoding for Gemini Live server frames.
//!
//! Every Windows Live consumer should decode a frame here before applying its
//! feature-specific state transitions. In particular, protocol signals are
//! recognized by their JSON location rather than by searching arbitrary text.

use base64::{Engine as _, engine::general_purpose};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveTextPart {
    pub text: String,
    pub thought: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveFunctionCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveGoAway {
    pub time_left: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveSessionResumption {
    pub handle: Option<String>,
    pub resumable: bool,
}

/// All protocol fields recognized in one Gemini Live server frame.
///
/// `turn_complete` and `generation_complete` remain distinct because some
/// state machines need a real turn boundary, while finite generation clients
/// may finish on either signal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveServerFrame {
    pub setup_complete: bool,
    pub server_content_present: bool,
    pub text_parts: Vec<LiveTextPart>,
    pub audio_chunks: Vec<Vec<u8>>,
    pub input_transcript: Option<String>,
    pub output_transcript: Option<String>,
    pub turn_complete: bool,
    pub generation_complete: bool,
    pub interrupted: bool,
    pub error: Option<String>,
    pub error_retryable: bool,
    pub tool_calls: Vec<LiveFunctionCall>,
    pub tool_call_present: bool,
    pub tool_cancellation_ids: Option<Vec<String>>,
    pub go_away: Option<LiveGoAway>,
    pub session_resumption: Option<LiveSessionResumption>,
    pub usage_metadata: Option<Value>,
    pub recognized: bool,
}

impl LiveServerFrame {
    /// Finite response clients may stop on either server completion signal.
    pub fn response_complete(&self) -> bool {
        self.turn_complete || self.generation_complete
    }

    /// Number of independently deliverable content observations in this frame.
    pub fn content_count(&self) -> usize {
        self.text_parts.len()
            + self.audio_chunks.len()
            + usize::from(self.input_transcript.is_some())
            + usize::from(self.output_transcript.is_some())
    }

    /// Whether a setup-phase frame carries protocol data that must survive the
    /// transition into the active session.
    pub fn has_post_setup_observation(&self) -> bool {
        self.server_content_present
            || self.tool_call_present
            || self.tool_cancellation_ids.is_some()
            || self.go_away.is_some()
            || self.session_resumption.is_some()
            || self.usage_metadata.is_some()
    }
}

pub fn parse_server_frame(message: &str) -> serde_json::Result<LiveServerFrame> {
    let root: Value = serde_json::from_str(message)?;
    let Some(root) = root.as_object() else {
        return Ok(LiveServerFrame::default());
    };

    let mut frame = LiveServerFrame {
        setup_complete: root.contains_key("setupComplete"),
        server_content_present: root.contains_key("serverContent"),
        tool_call_present: root.contains_key("toolCall"),
        recognized: [
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
        .any(|key| root.contains_key(*key)),
        ..LiveServerFrame::default()
    };

    if let Some(error) = root.get("error") {
        frame.error = protocol_error_message(error);
        frame.error_retryable = frame.error.is_some() && protocol_error_is_retryable(error);
    }

    if let Some(server_content) = root.get("serverContent") {
        frame.turn_complete =
            server_content.get("turnComplete").and_then(Value::as_bool) == Some(true);
        frame.generation_complete = server_content
            .get("generationComplete")
            .and_then(Value::as_bool)
            == Some(true);
        frame.interrupted =
            server_content.get("interrupted").and_then(Value::as_bool) == Some(true);
        frame.input_transcript = transcription(server_content, "inputTranscription");
        frame.output_transcript = transcription(server_content, "outputTranscription");

        if let Some(parts) = server_content
            .pointer("/modelTurn/parts")
            .and_then(Value::as_array)
        {
            for part in parts {
                if let Some(data) = part.pointer("/inlineData/data").and_then(Value::as_str)
                    && !data.trim().is_empty()
                    && let Ok(bytes) = general_purpose::STANDARD.decode(data)
                {
                    frame.audio_chunks.push(bytes);
                }
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.trim().is_empty()
                {
                    frame.text_parts.push(LiveTextPart {
                        text: text.to_string(),
                        thought: part.get("thought").and_then(Value::as_bool) == Some(true),
                    });
                }
            }
        }
    }

    if let Some(calls) = root
        .get("toolCall")
        .and_then(|tool_call| tool_call.get("functionCalls"))
        .and_then(Value::as_array)
    {
        frame.tool_calls.extend(calls.iter().map(|call| {
            LiveFunctionCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                name: call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                args: call.get("args").cloned().unwrap_or(Value::Null),
            }
        }));
    }

    if let Some(cancellation) = root.get("toolCallCancellation") {
        frame.tool_cancellation_ids = Some(
            cancellation
                .get("ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect(),
        );
    }

    if let Some(go_away) = root.get("goAway") {
        frame.go_away = Some(LiveGoAway {
            time_left: go_away
                .get("timeLeft")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }

    if let Some(resumption) = root.get("sessionResumptionUpdate") {
        frame.session_resumption = Some(LiveSessionResumption {
            handle: resumption
                .get("newHandle")
                .and_then(Value::as_str)
                .map(str::to_string),
            resumable: resumption.get("resumable").and_then(Value::as_bool) == Some(true),
        });
    }

    frame.usage_metadata = root.get("usageMetadata").cloned();
    Ok(frame)
}

fn transcription(server_content: &Value, field: &str) -> Option<String> {
    server_content
        .get(field)
        .and_then(|transcription| transcription.get("text"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

fn protocol_error_message(error: &Value) -> Option<String> {
    match error {
        Value::Null => None,
        Value::String(message) => (!message.trim().is_empty()).then(|| message.clone()),
        Value::Object(_) => error
            .get("message")
            .and_then(Value::as_str)
            .filter(|message| !message.trim().is_empty())
            .map(str::to_string)
            .or_else(|| Some(error.to_string())),
        _ => Some(error.to_string()),
    }
}

fn protocol_error_is_retryable(error: &Value) -> bool {
    let code = error.get("code").and_then(Value::as_i64);
    let status = error.get("status").and_then(Value::as_str);
    matches!(code, Some(408 | 429 | 500 | 502 | 503 | 504))
        || matches!(
            status,
            Some(
                "ABORTED" | "DEADLINE_EXCEEDED" | "INTERNAL" | "RESOURCE_EXHAUSTED" | "UNAVAILABLE"
            )
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_completion_requires_the_top_level_protocol_field() {
        for raw in [
            r#"{"error":{"message":"setupComplete failed"}}"#,
            r#"{"note":"waiting for setupComplete"}"#,
            r#"{"nested":{"setupComplete":{}}}"#,
        ] {
            assert!(!parse_server_frame(raw).unwrap().setup_complete, "{raw}");
        }

        assert!(
            parse_server_frame(r#"{"setupComplete":{}}"#)
                .unwrap()
                .setup_complete
        );
    }

    #[test]
    fn preserves_all_audio_text_and_transcript_fields() {
        let raw = r#"{
            "serverContent": {
                "modelTurn": {"parts": [
                    {"inlineData": {"data": "AQI="}},
                    {"inlineData": {"data": "not-base64"}},
                    {"inlineData": {"data": "AwQ="}},
                    {"text": "silent", "thought": true},
                    {"text": "visible"}
                ]},
                "inputTranscription": {"text": " heard "},
                "outputTranscription": {"text": " spoken "},
                "interrupted": true,
                "generationComplete": true
            }
        }"#;
        let frame = parse_server_frame(raw).unwrap();

        assert_eq!(frame.audio_chunks, vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(
            frame.text_parts,
            vec![
                LiveTextPart {
                    text: "silent".to_string(),
                    thought: true,
                },
                LiveTextPart {
                    text: "visible".to_string(),
                    thought: false,
                },
            ]
        );
        assert_eq!(frame.input_transcript.as_deref(), Some(" heard "));
        assert_eq!(frame.output_transcript.as_deref(), Some(" spoken "));
        assert_eq!(frame.content_count(), 6);
        assert!(frame.interrupted);
        assert!(frame.generation_complete);
        assert!(!frame.turn_complete);
        assert!(frame.response_complete());
    }

    #[test]
    fn malformed_inline_audio_does_not_count_as_content() {
        let frame = parse_server_frame(
            r#"{"serverContent":{"modelTurn":{"parts":[
                {"inlineData":{"data":"not-base64"}},
                {"inlineData":{"data":""}},
                {"inlineData":{"data":"  "}}
            ]}}}"#,
        )
        .unwrap();

        assert!(frame.audio_chunks.is_empty());
        assert_eq!(frame.content_count(), 0);
        assert!(frame.has_post_setup_observation());
    }

    #[test]
    fn blank_content_does_not_reset_lifecycle_activity() {
        let frame = parse_server_frame(
            r#"{"serverContent":{"modelTurn":{"parts":[{"text":"  "}]},"inputTranscription":{"text":"\n"},"outputTranscription":{"text":""}}}"#,
        )
        .unwrap();

        assert!(frame.text_parts.is_empty());
        assert!(frame.input_transcript.is_none());
        assert!(frame.output_transcript.is_none());
        assert_eq!(frame.content_count(), 0);
        assert!(frame.has_post_setup_observation());
    }

    #[test]
    fn protocol_error_normalization_matches_android_decoder() {
        assert_eq!(parse_server_frame(r#"{"error":null}"#).unwrap().error, None);
        assert_eq!(parse_server_frame(r#"{"error":""}"#).unwrap().error, None);
        assert_eq!(
            parse_server_frame(r#"{"error":"denied"}"#).unwrap().error,
            Some("denied".to_string())
        );
    }

    #[test]
    fn protocol_error_retryability_uses_structural_code_and_status() {
        let unavailable = parse_server_frame(
            r#"{"error":{"code":503,"status":"UNAVAILABLE","message":"retry"}}"#,
        )
        .unwrap();
        let exhausted =
            parse_server_frame(r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"later"}}"#)
                .unwrap();
        let invalid = parse_server_frame(
            r#"{"error":{"code":400,"status":"INVALID_ARGUMENT","message":"bad"}}"#,
        )
        .unwrap();

        assert!(unavailable.error_retryable);
        assert!(exhausted.error_retryable);
        assert!(!invalid.error_retryable);
    }

    #[test]
    fn combined_frame_matches_shared_parity_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/gemini-live-socket-protocol.json"
        )))
        .unwrap();
        let case = &fixture["combinedServerFrame"];
        let frame = parse_server_frame(&case["payload"].to_string()).unwrap();
        let expected = &case["expected"];

        assert_eq!(
            frame.input_transcript.as_deref(),
            expected["inputTranscript"].as_str()
        );
        assert_eq!(
            frame.output_transcript.as_deref(),
            expected["outputTranscript"].as_str()
        );
        let expected_audio = expected["audioPartsBase64"]
            .as_array()
            .unwrap()
            .iter()
            .map(|part| {
                general_purpose::STANDARD
                    .decode(part.as_str().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(frame.audio_chunks, expected_audio);
        let visible = frame
            .text_parts
            .iter()
            .filter(|part| !part.thought)
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>();
        let expected_visible = expected["visibleTextParts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|part| part.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(visible, expected_visible);
        assert_eq!(
            frame.response_complete(),
            expected["responseComplete"].as_bool().unwrap()
        );
        assert_eq!(
            frame.interrupted,
            expected["interrupted"].as_bool().unwrap()
        );
    }

    #[test]
    fn decodes_session_and_tool_protocol_fields() {
        let raw = r#"{
            "toolCall":{"functionCalls":[{"id":"c1","name":"act","args":{"x":1}}]},
            "toolCallCancellation":{"ids":["c1", 2]},
            "goAway":{"timeLeft":"5s"},
            "sessionResumptionUpdate":{"newHandle":"h1","resumable":true},
            "usageMetadata":{"totalTokenCount":9}
        }"#;
        let frame = parse_server_frame(raw).unwrap();

        assert_eq!(frame.tool_calls.len(), 1);
        assert_eq!(frame.tool_calls[0].id, "c1");
        assert_eq!(frame.tool_calls[0].name, "act");
        assert_eq!(frame.tool_calls[0].args["x"], 1);
        assert_eq!(frame.tool_cancellation_ids, Some(vec!["c1".to_string()]));
        assert_eq!(frame.go_away.unwrap().time_left, "5s");
        assert_eq!(
            frame.session_resumption,
            Some(LiveSessionResumption {
                handle: Some("h1".to_string()),
                resumable: true,
            })
        );
        assert_eq!(frame.usage_metadata.unwrap()["totalTokenCount"], 9);
        assert!(frame.recognized);
    }
}
