use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

pub const PROTOCOL_VERSION: u32 = 1;
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;
pub const MAX_JSON_BYTES: usize = 256 * 1024;
pub const MAX_REQUEST_LINE_BYTES: usize = MAX_JSON_BYTES + 1;
pub const MAX_SCRIPT_BYTES: usize = 128 * 1024;
pub const MAX_PATH_BYTES: usize = 32 * 1024;
pub const MAX_DECODED_AUDIO_BYTES: u64 = 512 * 1024 * 1024;
pub const RESPONSE_PREFIX: &str = "SGT_RECORDER_IPC ";
pub const SETTINGS_PREFIX: &str = "SGT_RECORDER_SETTINGS ";
pub const PROGRESS_PROTOCOL_VERSION: u32 = 1;
pub const MAX_PROGRESS_KEYS: usize = 64;
pub const MAX_PROGRESS_DATAGRAM_BYTES: usize = 16 * 1024;
pub const SETTINGS_PROTOCOL_VERSION: u32 = 2;
/// Requires a host-owned helper job before startup and verified helper retirement on shutdown.
pub const SHUTDOWN_PROTOCOL_VERSION: u32 = 2;
pub const MAX_RESPONSE_LINE_BYTES: usize = RESPONSE_PREFIX.len() + MAX_JSON_BYTES + 1;

/// Audio file types, lowercase and without the dot, that the host routes and
/// decodes and the recorder imports under their own extension. One list, so a
/// file the host hands to the recorder is never refused or relabeled there.
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "m4b", "aac", "alac", "aiff", "aif", "opus", "mka",
];

/// The entry of [`SUPPORTED_AUDIO_EXTENSIONS`] matching `extension`, ignoring case.
pub fn supported_audio_extension(extension: &str) -> Option<&'static str> {
    SUPPORTED_AUDIO_EXTENSIONS
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(extension))
}

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
    SettingsAck {
        event_id: u64,
        settings: Option<RecorderSettings>,
        error: Option<String>,
    },
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
    DecodeAudio {
        input_path: String,
        output_path: String,
    },
    NotifyAudioReleased {
        reason: String,
    },
    Cleanup,
    PrepareShutdown,
    CancelShutdown,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecorderHotkey {
    pub code: u32,
    pub modifiers: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecorderSettings {
    pub hotkeys: Vec<RecorderHotkey>,
    pub project_limit: usize,
    pub upload_limit: usize,
    pub window_size: (i32, i32),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum SettingsChange {
    Snapshot,
    AddHotkey(RecorderHotkey),
    RemoveHotkey(RecorderHotkey),
    ProjectLimit(usize),
    UploadLimit(usize),
    WindowSize((i32, i32)),
    /// The recorder is (or stops) recording a new binding, so the host pauses
    /// (or resumes) its global hotkeys. Nothing is saved; the ack is a snapshot.
    BindingCapture(bool),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsEvent {
    pub protocol_version: u32,
    pub token: String,
    pub event_id: u64,
    pub change: SettingsChange,
}

impl SettingsEvent {
    pub fn validate(&self, expected_token: &str, previous_event_id: u64) -> Result<(), String> {
        if self.protocol_version != SETTINGS_PROTOCOL_VERSION
            || !tokens_equal(&self.token, expected_token)
            || self.event_id == 0
            || self.event_id <= previous_event_id
        {
            return Err("invalid or stale recorder settings event".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressEntry {
    pub operation_id: u64,
    pub key: String,
    pub percent: Option<f32>,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressEvent {
    pub protocol_version: u32,
    pub token: String,
    pub event_id: u64,
    pub entries: Vec<ProgressEntry>,
}

pub fn valid_progress_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
}

impl ProgressEvent {
    pub fn validate(&self, expected_token: &str, previous_event_id: u64) -> Result<(), String> {
        if self.protocol_version != PROGRESS_PROTOCOL_VERSION
            || !tokens_equal(&self.token, expected_token)
            || self.event_id == 0
            || self.event_id <= previous_event_id
            || self.entries.len() > MAX_PROGRESS_KEYS
        {
            return Err("invalid or stale recorder progress event".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for entry in &self.entries {
            if entry.operation_id == 0
                || !seen.insert(entry.operation_id)
                || !valid_progress_key(&entry.key)
                || entry
                    .percent
                    .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
                || (entry.percent.is_some() != (entry.revision > 0))
            {
                return Err("invalid recorder progress entry".to_string());
            }
        }
        Ok(())
    }
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
            Self::SettingsAck {
                event_id,
                settings,
                error,
            } => {
                if *event_id == 0
                    || settings.is_some() == error.is_some()
                    || error
                        .as_ref()
                        .is_some_and(|error| error.is_empty() || error.len() > 4096)
                {
                    return Err("invalid recorder settings acknowledgement".to_string());
                }
            }
            Self::EvaluateScript { script } => {
                validate_text(script, MAX_SCRIPT_BYTES, "script")?;
            }
            Self::QueueVideoDrop { path, .. }
            | Self::QueueAudioDrop { path }
            | Self::QueueSubtitleDrop { path } => validate_path(path)?,
            Self::DecodeAudio {
                input_path,
                output_path,
            } => {
                validate_path(input_path)?;
                validate_path(output_path)?;
            }
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
    fn progress_snapshot_rejects_stale_spoofed_duplicate_and_unbounded_values() {
        let mut event = ProgressEvent {
            protocol_version: PROGRESS_PROTOCOL_VERSION,
            token: token(),
            event_id: 4,
            entries: vec![ProgressEntry {
                operation_id: 1,
                key: "model-one".into(),
                percent: Some(45.0),
                revision: 1,
            }],
        };
        assert!(event.validate(&token(), 3).is_ok());
        assert!(event.validate(&token(), 4).is_err());
        assert!(event.validate(&"00".repeat(TOKEN_BYTES), 3).is_err());
        event.entries.push(ProgressEntry {
            operation_id: 1,
            key: "model-one".into(),
            percent: None,
            revision: 0,
        });
        assert!(event.validate(&token(), 3).is_err());
        event.entries.pop();
        event.entries[0].percent = Some(f32::NAN);
        assert!(event.validate(&token(), 3).is_err());
        event.entries[0].percent = Some(101.0);
        assert!(event.validate(&token(), 3).is_err());
        event.entries[0].percent = None;
        event.entries[0].revision = 0;
        event.entries[0].key = "bad/key".into();
        assert!(event.validate(&token(), 3).is_err());
        event.entries = (0..=MAX_PROGRESS_KEYS)
            .map(|i| ProgressEntry {
                operation_id: i as u64 + 1,
                key: format!("model-{i}"),
                percent: None,
                revision: 0,
            })
            .collect();
        assert!(event.validate(&token(), 3).is_err());
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
    fn settings_events_require_current_session_token_and_fresh_id() {
        let mut event = SettingsEvent {
            protocol_version: SETTINGS_PROTOCOL_VERSION,
            token: token(),
            event_id: 7,
            change: SettingsChange::Snapshot,
        };
        assert!(event.validate(&token(), 6).is_ok());
        assert!(event.validate(&token(), 7).is_err());
        assert!(event.validate(&"00".repeat(TOKEN_BYTES), 0).is_err());
        event.protocol_version += 1;
        assert!(event.validate(&token(), 0).is_err());
    }

    #[test]
    fn binding_capture_round_trips_as_a_settings_change() {
        let wire = serde_json::to_string(&SettingsChange::BindingCapture(true)).unwrap();
        assert_eq!(wire, r#"{"type":"binding-capture","payload":true}"#);
        assert!(matches!(
            serde_json::from_str(&wire).unwrap(),
            SettingsChange::BindingCapture(true)
        ));
    }

    #[test]
    fn settings_ack_requires_exactly_one_outcome() {
        let ack = |event_id, settings, error| Command::SettingsAck {
            event_id,
            settings,
            error,
        };
        let settings = RecorderSettings {
            hotkeys: vec![],
            project_limit: 50,
            upload_limit: 8,
            window_size: (1000, 700),
        };
        assert!(ack(1, Some(settings.clone()), None).validate().is_ok());
        assert!(ack(1, None, Some("failed".into())).validate().is_ok());
        assert!(ack(0, Some(settings.clone()), None).validate().is_err());
        assert!(ack(1, None, None).validate().is_err());
        assert!(
            ack(1, Some(settings), Some("failed".into()))
                .validate()
                .is_err()
        );
    }

    #[test]
    fn framed_limits_account_for_prefix_and_newline() {
        assert_eq!(MAX_REQUEST_LINE_BYTES, MAX_JSON_BYTES + 1);
        assert_eq!(
            MAX_RESPONSE_LINE_BYTES,
            RESPONSE_PREFIX.len() + MAX_JSON_BYTES + 1
        );
    }

    #[test]
    fn audio_decode_requires_both_bounded_paths() {
        assert!(
            Command::DecodeAudio {
                input_path: "C:\\input.ogg".to_string(),
                output_path: "C:\\output.wav".to_string(),
            }
            .validate()
            .is_ok()
        );
        assert!(
            Command::DecodeAudio {
                input_path: String::new(),
                output_path: "C:\\output.wav".to_string(),
            }
            .validate()
            .is_err()
        );
    }
}
