use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

pub const PROTOCOL_VERSION: u32 = 1;
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;
pub const MAX_JSON_BYTES: usize = 256 * 1024;
pub const MAX_REQUEST_LINE_BYTES: usize = MAX_JSON_BYTES + 1;
pub const MAX_SCRIPT_BYTES: usize = 128 * 1024;
pub const MAX_PATH_BYTES: usize = 32 * 1024;
pub const RESPONSE_PREFIX: &str = "SGT_RECORDER_IPC ";
pub const MAX_RESPONSE_LINE_BYTES: usize = RESPONSE_PREFIX.len() + MAX_JSON_BYTES + 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub token: String,
    pub request_id: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum Command {
    Ping,
    Show,
    Toggle,
    UpdateSettings,
    EvaluateScript {
        script: String,
    },
    QueueVideoDrop {
        path: String,
        action: VideoDropAction,
    },
    QueueAudioDrop {
        path: String,
    },
    QueueSubtitleDrop {
        path: String,
    },
    NotifyAudioReleased {
        reason: String,
    },
    Cleanup,
    ExportReplay {
        path: String,
        runs: u16,
        keep_outputs: bool,
    },
    GtNarrationTest {
        input_wav: String,
        target_language: String,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoDropAction {
    WorkRecord,
    GenerateSubtitles,
}

impl VideoDropAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkRecord => "work-record",
            Self::GenerateSubtitles => "generate-subtitles",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "work-record" => Some(Self::WorkRecord),
            "generate-subtitles" => Some(Self::GenerateSubtitles),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub protocol_version: u32,
    pub token: String,
    pub request_id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl Request {
    pub fn validate(&self, expected_token: &str, previous_request_id: u64) -> Result<(), String> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err("unsupported recorder protocol version".to_string());
        }
        if !tokens_equal(&self.token, expected_token) {
            return Err("recorder request authentication failed".to_string());
        }
        if self.request_id == 0 || self.request_id <= previous_request_id {
            return Err("recorder request id is stale".to_string());
        }
        self.command.validate()
    }
}

impl Response {
    pub fn validate(&self, expected_token: &str, expected_request_id: u64) -> Result<(), String> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err("unsupported recorder protocol version".to_string());
        }
        if !tokens_equal(&self.token, expected_token) {
            return Err("recorder response authentication failed".to_string());
        }
        if expected_request_id == 0 || self.request_id != expected_request_id {
            return Err("recorder response id does not match the request".to_string());
        }
        Ok(())
    }
}

impl Command {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::EvaluateScript { script } => {
                validate_text(script, MAX_SCRIPT_BYTES, "script")?;
            }
            Self::QueueVideoDrop { path, .. }
            | Self::QueueAudioDrop { path }
            | Self::QueueSubtitleDrop { path } => validate_path(path)?,
            Self::NotifyAudioReleased { reason } => validate_text(reason, 1024, "reason")?,
            Self::ExportReplay { path, runs, .. } => {
                validate_path(path)?;
                if *runs == 0 || *runs > 100 {
                    return Err("recorder replay run count is invalid".to_string());
                }
            }
            Self::GtNarrationTest {
                input_wav,
                target_language,
            } => {
                validate_path(input_wav)?;
                validate_text(target_language, 128, "target language")?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn valid_token(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn tokens_equal(left: &str, right: &str) -> bool {
    valid_token(left) && valid_token(right) && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn validate_path(path: &str) -> Result<(), String> {
    validate_text(path, MAX_PATH_BYTES, "path")?;
    if path.contains('\0') {
        return Err("recorder path contains NUL".to_string());
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > maximum {
        return Err(format!("recorder {label} length is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> String {
        "01".repeat(TOKEN_BYTES)
    }

    #[test]
    fn rejects_stale_ids_wrong_tokens_and_oversized_payloads() {
        let mut request = Request {
            protocol_version: PROTOCOL_VERSION,
            token: token(),
            request_id: 2,
            command: Command::Show,
        };
        assert!(request.validate(&token(), 1).is_ok());
        assert!(request.validate(&token(), 2).is_err());
        request.token = "00".repeat(TOKEN_BYTES);
        assert!(request.validate(&token(), 1).is_err());
        request.token = token();
        request.command = Command::EvaluateScript {
            script: "x".repeat(MAX_SCRIPT_BYTES + 1),
        };
        assert!(request.validate(&token(), 1).is_err());
    }

    #[test]
    fn response_validation_rejects_wrong_tokens_versions_and_request_ids() {
        let mut response = Response {
            protocol_version: PROTOCOL_VERSION,
            token: token(),
            request_id: 7,
            result: Some(serde_json::json!({ "status": "ok" })),
            error: None,
        };
        assert!(response.validate(&token(), 7).is_ok());
        assert!(response.validate(&"00".repeat(TOKEN_BYTES), 7).is_err());
        assert!(response.validate(&token(), 6).is_err());
        response.protocol_version += 1;
        assert!(response.validate(&token(), 7).is_err());
    }

    #[test]
    fn constant_time_token_helper_requires_exact_hex_tokens() {
        assert!(tokens_equal(&token(), &token()));
        assert!(!tokens_equal(&token(), &"00".repeat(TOKEN_BYTES)));
        assert!(!tokens_equal(&token(), "not-a-token"));
    }

    #[test]
    fn serde_contract_denies_unknown_fields() {
        let raw = format!(
            r#"{{"protocolVersion":1,"token":"{}","requestId":1,"command":{{"type":"show"}},"extra":1}}"#,
            token()
        );
        assert!(serde_json::from_str::<Request>(&raw).is_err());
    }

    #[test]
    fn framed_limits_account_for_prefix_and_newline() {
        assert_eq!(MAX_REQUEST_LINE_BYTES, MAX_JSON_BYTES + 1);
        assert_eq!(
            MAX_RESPONSE_LINE_BYTES,
            RESPONSE_PREFIX.len() + MAX_JSON_BYTES + 1
        );
    }
}
