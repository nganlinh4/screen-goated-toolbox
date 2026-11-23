use serde_json::Value;
use sgt_computer_control_protocol::{
    Command, HostPrivilege, MAX_MCP_INTEGRATIONS, MAX_MCP_TOOLS, SetupRequest,
};

use super::ConnSnapshot;

#[derive(Clone, Debug)]
pub(super) struct CatalogCapacity {
    detail: String,
}

impl std::fmt::Display for CatalogCapacity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "MCP catalog capacity exceeded: {}. Remove an integration or reduce its advertised catalog before retrying; no reduced catalog was activated",
            self.detail
        )
    }
}

impl std::error::Error for CatalogCapacity {}

pub(super) fn check_snapshot(snapshot: &ConnSnapshot) -> anyhow::Result<()> {
    let tools: usize = snapshot.iter().map(|(_, _, tools)| tools.len()).sum();
    let detail = if snapshot.len() > MAX_MCP_INTEGRATIONS {
        Some(format!(
            "{} integrations, limit {MAX_MCP_INTEGRATIONS}",
            snapshot.len()
        ))
    } else if tools > MAX_MCP_TOOLS {
        Some(format!("{tools} tools, limit {MAX_MCP_TOOLS}"))
    } else {
        None
    };
    match detail {
        Some(detail) => Err(CatalogCapacity { detail }.into()),
        None => Ok(()),
    }
}

pub(super) fn check_declarations(declarations: &[Value]) -> anyhow::Result<()> {
    // Use the protocol's own admission contract, including exact JSON escaping
    // and array punctuation, instead of maintaining another byte limit here.
    let command = Command::BuildSetup(SetupRequest {
        privilege: HostPrivilege::Standard,
        voice_mode: true,
        search_enabled: true,
        integration_declarations: declarations.to_vec(),
    });
    command.validate().map_err(|detail| {
        CatalogCapacity {
            detail: format!(
                "{detail} ({} encoded bytes)",
                serde_json::to_vec(declarations).map_or(0, |bytes| bytes.len())
            ),
        }
        .into()
    })
}

#[derive(Clone)]
pub(super) struct CatalogIssue {
    pub integration_ids: Vec<String>,
    pub detail: String,
}

impl CatalogIssue {
    pub fn from_snapshot(snapshot: &ConnSnapshot, error: &CatalogCapacity) -> Self {
        Self {
            integration_ids: snapshot.iter().map(|(id, _, _)| id.clone()).collect(),
            detail: error.to_string(),
        }
    }
}
