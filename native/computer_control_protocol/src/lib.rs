use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;
pub const MAX_JSON_BYTES: usize = 256 * 1024;
pub const MAX_REQUEST_LINE_BYTES: usize = MAX_JSON_BYTES + 1;
pub const RESPONSE_PREFIX: &str = "SGT_CC_ENGINE_IPC ";
pub const MAX_RESPONSE_LINE_BYTES: usize = RESPONSE_PREFIX.len() + MAX_JSON_BYTES + 1;
pub const MAX_PROVIDER_FRAME_BYTES: usize = 224 * 1024;
pub const MAX_INTEGRATION_DECLARATIONS: usize = 128;
pub const MAX_MCP_INTEGRATIONS: usize = 32;
pub const MAX_MCP_TOOLS: usize = MAX_INTEGRATION_DECLARATIONS;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub token: String,
    pub request_id: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    Handshake { host_version: String },
    BuildSetup(SetupRequest),
    NormalizeMcpCatalog(McpCatalogRequest),
    ParseProviderFrame { frame: String },
    Shutdown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostPrivilege {
    Standard,
    Elevated,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupRequest {
    pub privilege: HostPrivilege,
    pub voice_mode: bool,
    pub search_enabled: bool,
    pub integration_declarations: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub protocol_version: u32,
    pub token: String,
    pub request_id: u64,
    pub output: Option<Output>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Output {
    Handshake {
        engine_version: String,
        architecture: String,
    },
    Setup(SetupPlan),
    McpCatalog(McpCatalogPlan),
    ProviderEvents {
        events: Vec<ProviderEvent>,
    },
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupPlan {
    pub system_instruction: String,
    pub tools: Value,
    pub realtime_input_config: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCatalogRequest {
    pub integrations: Vec<McpIntegrationSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpIntegrationSnapshot {
    pub id: String,
    pub display_name: String,
    pub tools: Vec<McpToolSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpToolSnapshot {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCatalogPlan {
    pub normalized: Vec<McpNormalizedTool>,
    pub quarantined: Vec<McpQuarantinedTool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpNormalizedTool {
    pub integration_index: u32,
    pub tool_index: u32,
    pub declaration: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpQuarantinedTool {
    pub integration_index: u32,
    pub tool_index: u32,
    pub reason: String,
    pub observed: u64,
    pub limit: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProviderEvent {
    SetupComplete,
    AudioPcm16 {
        data_base64: String,
    },
    ModelText {
        text: String,
    },
    Thought {
        text: String,
    },
    InputTranscript {
        text: String,
    },
    OutputTranscript {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    ToolCancellation {
        ids: Vec<String>,
    },
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
    Usage {
        metadata: Value,
    },
    Other {
        summary: String,
    },
}

impl Request {
    pub fn validate(&self, expected_token: &str, previous_request_id: u64) -> Result<(), String> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err("unsupported Computer Control engine protocol version".to_string());
        }
        if !valid_token(&self.token) || !constant_time_eq(&self.token, expected_token) {
            return Err("Computer Control engine request authentication failed".to_string());
        }
        if self.request_id == 0 || self.request_id <= previous_request_id {
            return Err("Computer Control engine request id is stale".to_string());
        }
        self.command.validate()
    }
}

impl Command {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Handshake { host_version } => validate_text(host_version, 128, "host version"),
            Self::BuildSetup(request) => request.validate(),
            Self::NormalizeMcpCatalog(request) => request.validate(),
            Self::ParseProviderFrame { frame } => {
                validate_text(frame, MAX_PROVIDER_FRAME_BYTES, "provider frame")
            }
            Self::Shutdown => Ok(()),
        }
    }
}

impl McpCatalogRequest {
    fn validate(&self) -> Result<(), String> {
        if self.integrations.len() > MAX_MCP_INTEGRATIONS {
            return Err("too many Computer Control MCP integrations".to_string());
        }
        let mut tool_count = 0_usize;
        for integration in &self.integrations {
            validate_text(&integration.id, 512, "MCP integration id")?;
            validate_text(&integration.display_name, 512, "MCP integration name")?;
            tool_count = tool_count
                .checked_add(integration.tools.len())
                .ok_or_else(|| "too many Computer Control MCP tools".to_string())?;
            if tool_count > MAX_MCP_TOOLS {
                return Err("too many Computer Control MCP tools".to_string());
            }
            for tool in &integration.tools {
                validate_text(&tool.name, 512, "MCP tool name")?;
                if tool.description.len() > 64 * 1024 || tool.description.contains('\0') {
                    return Err("Computer Control MCP tool description is invalid".to_string());
                }
                if !tool.input_schema.is_object() {
                    return Err("Computer Control MCP tool schema is invalid".to_string());
                }
            }
        }
        Ok(())
    }
}

impl SetupRequest {
    fn validate(&self) -> Result<(), String> {
        if self.integration_declarations.len() > MAX_INTEGRATION_DECLARATIONS {
            return Err("too many Computer Control integration declarations".to_string());
        }
        let encoded = serde_json::to_vec(&self.integration_declarations)
            .map_err(|error| format!("invalid integration declarations: {error}"))?;
        if encoded.len() > 96 * 1024 {
            return Err("Computer Control integration declarations are too large".to_string());
        }
        Ok(())
    }
}

impl Response {
    pub fn validate(&self, expected_token: &str, request_id: u64) -> Result<(), String> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err("unsupported Computer Control engine response version".to_string());
        }
        if !valid_token(&self.token) || !constant_time_eq(&self.token, expected_token) {
            return Err("Computer Control engine response authentication failed".to_string());
        }
        if self.request_id != request_id {
            return Err("Computer Control engine response id does not match request".to_string());
        }
        if self.output.is_some() == self.error.is_some() {
            return Err("Computer Control engine response must contain one outcome".to_string());
        }
        Ok(())
    }
}

pub fn valid_token(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0_u8, |different, (left, right)| different | (left ^ right))
        == 0
}

fn validate_text(value: &str, maximum: usize, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(format!("Computer Control engine {label} length is invalid"));
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
    fn request_authentication_and_monotonic_ids_fail_closed() {
        let request = Request {
            protocol_version: PROTOCOL_VERSION,
            token: token(),
            request_id: 2,
            command: Command::Handshake {
                host_version: "1.0.0".to_string(),
            },
        };
        assert!(request.validate(&token(), 1).is_ok());
        assert!(request.validate(&token(), 2).is_err());
        assert!(request.validate(&"00".repeat(TOKEN_BYTES), 1).is_err());
    }

    #[test]
    fn strict_request_schema_rejects_unknown_fields() {
        let raw = format!(
            r#"{{"protocolVersion":1,"token":"{}","requestId":1,"command":{{"type":"shutdown"}},"extra":true}}"#,
            token()
        );
        assert!(serde_json::from_str::<Request>(&raw).is_err());

        let nested = format!(
            r#"{{"protocolVersion":1,"token":"{}","requestId":1,"command":{{"type":"handshake","payload":{{"hostVersion":"1.0.0","extra":true}}}}}}"#,
            token()
        );
        assert!(serde_json::from_str::<Request>(&nested).is_err());
    }

    #[test]
    fn enum_payload_fields_use_the_protocol_casing() {
        let command = serde_json::to_value(Command::Handshake {
            host_version: "1.0.0".to_string(),
        })
        .unwrap();
        assert_eq!(command["payload"]["hostVersion"], "1.0.0");
        assert!(command["payload"].get("host_version").is_none());

        let output = serde_json::to_value(Output::ProviderEvents {
            events: vec![ProviderEvent::AudioPcm16 {
                data_base64: "AQI=".to_string(),
            }],
        })
        .unwrap();
        assert_eq!(
            output["payload"]["events"][0]["payload"]["dataBase64"],
            "AQI="
        );
    }

    #[test]
    fn frame_and_catalog_limits_are_enforced() {
        let oversized = Command::ParseProviderFrame {
            frame: "x".repeat(MAX_PROVIDER_FRAME_BYTES + 1),
        };
        assert!(oversized.validate().is_err());
        let setup = SetupRequest {
            privilege: HostPrivilege::Standard,
            voice_mode: true,
            search_enabled: true,
            integration_declarations: vec![Value::Null; MAX_INTEGRATION_DECLARATIONS + 1],
        };
        assert!(Command::BuildSetup(setup).validate().is_err());

        let tool = McpToolSnapshot {
            name: "future_capability".to_string(),
            description: "Future operation".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let catalog = McpCatalogRequest {
            integrations: vec![McpIntegrationSnapshot {
                id: "future_provider".to_string(),
                display_name: "Future Provider".to_string(),
                tools: vec![tool; MAX_MCP_TOOLS + 1],
            }],
        };
        assert!(Command::NormalizeMcpCatalog(catalog).validate().is_err());
    }
}
