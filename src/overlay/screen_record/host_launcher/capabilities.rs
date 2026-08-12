use std::sync::atomic::AtomicBool;

use anyhow::{Context as _, Result};

use crate::component_registry::external_tools::{ExternalTool, ExternalToolUse};

#[derive(Debug)]
pub(super) struct MissingExternalCapability {
    pub(super) tool: ExternalTool,
}

impl std::fmt::Display for MissingExternalCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "recorder worker requires {}", self.tool.id())
    }
}

impl std::error::Error for MissingExternalCapability {}

pub(super) fn prepare_external_capabilities(
    cancelled: &AtomicBool,
    requested: Option<ExternalTool>,
) -> Result<Vec<ExternalToolUse>> {
    let mut tools =
        crate::component_registry::capabilities::RECORDER_REQUIRED_EXTERNAL_TOOLS.to_vec();
    if let Some(requested) = requested
        && !tools.contains(&requested)
    {
        tools.push(requested);
    }
    tools
        .into_iter()
        .map(|tool| {
            crate::component_registry::capabilities::resolve_external_tool_with_badge(
                tool, cancelled, None,
            )
            .with_context(|| format!("prepare recorder capability {}", tool.id()))
        })
        .collect()
}
