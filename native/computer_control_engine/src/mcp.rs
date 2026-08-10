use std::collections::HashSet;

use anyhow::{Context as _, Result};
use serde_json::json;
use sgt_computer_control_protocol::{
    McpCatalogPlan, McpCatalogRequest, McpNormalizedTool, McpQuarantinedTool,
};

mod schema;

pub(super) fn normalize(request: McpCatalogRequest) -> Result<McpCatalogPlan> {
    let tool_count = request
        .integrations
        .iter()
        .map(|integration| integration.tools.len())
        .sum();
    let mut normalized = Vec::with_capacity(tool_count);
    let mut quarantined = Vec::new();
    let mut seen = HashSet::with_capacity(tool_count);
    for (integration_index, integration) in request.integrations.into_iter().enumerate() {
        for (tool_index, tool) in integration.tools.into_iter().enumerate() {
            let integration_index = u32::try_from(integration_index)
                .context("Computer Control MCP integration index overflow")?;
            let tool_index =
                u32::try_from(tool_index).context("Computer Control MCP tool index overflow")?;
            let parameters = match schema::sanitize_schema(&tool.input_schema) {
                Ok(parameters) => parameters,
                Err(issue) => {
                    quarantined.push(McpQuarantinedTool {
                        integration_index,
                        tool_index,
                        reason: issue.reason.to_string(),
                        observed: usize_to_u64(issue.observed),
                        limit: usize_to_u64(issue.limit),
                    });
                    continue;
                }
            };
            let name = schema::unique_decl_name(&integration.id, &tool.name, &mut seen);
            normalized.push(McpNormalizedTool {
                integration_index,
                tool_index,
                declaration: json!({
                    "name": name,
                    "description": schema::bounded_prose(&format!(
                        "{}: {}",
                        integration.display_name, tool.description
                    )),
                    "parameters": parameters,
                }),
            });
        }
    }
    Ok(McpCatalogPlan {
        normalized,
        quarantined,
    })
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sgt_computer_control_protocol::{McpIntegrationSnapshot, McpToolSnapshot};

    fn request(schema: serde_json::Value) -> McpCatalogRequest {
        McpCatalogRequest {
            integrations: vec![McpIntegrationSnapshot {
                id: "future/provider".to_string(),
                display_name: "Future Provider".to_string(),
                tools: vec![McpToolSnapshot {
                    name: "future tool".to_string(),
                    description: "Future capability".to_string(),
                    input_schema: schema,
                }],
            }],
        }
    }

    #[test]
    fn every_compatible_tool_is_normalized_and_source_indexed() {
        let plan = normalize(request(json!({
            "type": "object",
            "required": ["value"],
            "properties": {"value": {"type": "string"}}
        })))
        .unwrap();
        assert!(plan.quarantined.is_empty());
        assert_eq!(plan.normalized.len(), 1);
        assert_eq!(plan.normalized[0].integration_index, 0);
        assert_eq!(plan.normalized[0].tool_index, 0);
        assert_eq!(
            plan.normalized[0].declaration["parameters"]["required"],
            json!(["value"])
        );
    }

    #[test]
    fn incompatible_tool_is_explicitly_accounted_for() {
        let mut deep = json!({"type": "string"});
        for _ in 0..32 {
            deep = json!({"type": "array", "items": deep});
        }
        let plan = normalize(request(deep)).unwrap();
        assert!(plan.normalized.is_empty());
        assert_eq!(plan.quarantined.len(), 1);
        assert_eq!(plan.quarantined[0].reason, "schema_depth");
    }
}
