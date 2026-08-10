use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use sgt_computer_control_protocol::{
    McpCatalogPlan, McpCatalogRequest, McpIntegrationSnapshot, McpToolSnapshot,
};

use super::super::catalog;
use super::{ConnSnapshot, ToolRoute, ToolSnapshot};

pub(super) fn engine_declarations(
    snapshot: &ConnSnapshot,
) -> anyhow::Result<(Vec<Value>, HashMap<String, ToolRoute>)> {
    let integrations = snapshot
        .iter()
        .map(|(id, _, tools)| McpIntegrationSnapshot {
            id: id.clone(),
            display_name: catalog::get(id)
                .map(|integration| integration.display_name)
                .unwrap_or(id.as_str())
                .to_string(),
            tools: tools
                .iter()
                .map(|tool| McpToolSnapshot {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                })
                .collect(),
        })
        .collect();
    let plan =
        crate::overlay::computer_control::engine::normalize_mcp_catalog(McpCatalogRequest {
            integrations,
        })?;
    apply_normalized_catalog(snapshot, plan)
}

pub(super) fn apply_normalized_catalog(
    snapshot: &ConnSnapshot,
    plan: McpCatalogPlan,
) -> anyhow::Result<(Vec<Value>, HashMap<String, ToolRoute>)> {
    let source_count: usize = snapshot.iter().map(|(_, _, tools)| tools.len()).sum();
    if plan.normalized.len() + plan.quarantined.len() != source_count {
        anyhow::bail!("Computer Control engine returned incomplete MCP tool accounting");
    }
    let mut declarations = Vec::with_capacity(plan.normalized.len());
    let mut routes = HashMap::with_capacity(plan.normalized.len());
    let mut accounted = HashSet::with_capacity(source_count);
    for normalized in plan.normalized {
        let (integration_index, tool_index, source) = source_tool(
            snapshot,
            normalized.integration_index,
            normalized.tool_index,
        )?;
        if !accounted.insert((integration_index, tool_index)) {
            anyhow::bail!("Computer Control engine returned duplicate MCP tool accounting");
        }
        let declaration = normalized
            .declaration
            .as_object()
            .filter(|declaration| declaration.len() == 3)
            .ok_or_else(|| {
                anyhow::anyhow!("Computer Control engine returned invalid MCP declaration")
            })?;
        let name = declaration
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| {
                name.starts_with("mcp__")
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Computer Control engine returned invalid MCP tool name")
            })?;
        declaration
            .get("description")
            .and_then(Value::as_str)
            .filter(|description| {
                !description.is_empty()
                    && description.chars().count() <= 513
                    && !description.chars().any(char::is_control)
            })
            .ok_or_else(|| anyhow::anyhow!("Computer Control engine returned invalid MCP prose"))?;
        let parameters = declaration
            .get("parameters")
            .filter(|parameters| parameters.is_object())
            .ok_or_else(|| {
                anyhow::anyhow!("Computer Control engine returned invalid MCP schema")
            })?;
        if serde_json::to_vec(parameters)?.len() > 96 * 1_024 || routes.contains_key(name) {
            anyhow::bail!("Computer Control engine returned invalid MCP declaration bounds");
        }
        let (integration_id, connection_token, _) = &snapshot[integration_index];
        routes.insert(
            name.to_string(),
            ToolRoute {
                integration_id: integration_id.clone(),
                tool_name: source.name.clone(),
                annotations: source.annotations,
                connection_token: *connection_token,
            },
        );
        declarations.push(Value::Object(declaration.clone()));
    }
    for quarantine in plan.quarantined {
        let (integration_index, tool_index, source) = source_tool(
            snapshot,
            quarantine.integration_index,
            quarantine.tool_index,
        )?;
        if !accounted.insert((integration_index, tool_index))
            || quarantine.reason.is_empty()
            || quarantine.reason.len() > 64
            || !quarantine
                .reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            anyhow::bail!("Computer Control engine returned invalid MCP quarantine accounting");
        }
        let integration_id = &snapshot[integration_index].0;
        crate::overlay::computer_control::overlay::push_log(format!(
            "[mcp] quarantined unrepresentable tool schema: {integration_id}/{} ({}, observed {}, limit {})",
            source.name, quarantine.reason, quarantine.observed, quarantine.limit
        ));
        crate::overlay::computer_control::telemetry::typed_error(
            "ERR_MCP_TOOL_SCHEMA_UNREPRESENTABLE",
            "mcp",
            "an MCP tool was omitted because its input schema exceeds provider-wire bounds",
            json!({
                "integration_id": integration_id,
                "tool_name": source.name,
                "reason": quarantine.reason,
                "observed": quarantine.observed,
                "limit": quarantine.limit,
            }),
        );
    }
    if accounted.len() != source_count {
        anyhow::bail!("Computer Control engine omitted an MCP tool");
    }
    Ok((declarations, routes))
}

fn source_tool(
    snapshot: &ConnSnapshot,
    integration_index: u32,
    tool_index: u32,
) -> anyhow::Result<(usize, usize, &ToolSnapshot)> {
    let integration_index = usize::try_from(integration_index)?;
    let tool_index = usize::try_from(tool_index)?;
    let source = snapshot
        .get(integration_index)
        .and_then(|(_, _, tools)| tools.get(tool_index))
        .ok_or_else(|| {
            anyhow::anyhow!("Computer Control engine returned an invalid MCP source index")
        })?;
    Ok((integration_index, tool_index, source))
}
