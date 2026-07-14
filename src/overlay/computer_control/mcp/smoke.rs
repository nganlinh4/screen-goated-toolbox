use super::catalog;
use super::client::McpClient;
use super::support;
use serde_json::{Value, json};

/// Headless smoke test (`--cc-mcp-test <id>`): spawn the catalog's server for `id`,
/// list its tools, then either call an explicit tool or run the generic semantic
/// health probe. Verifies the stdio JSON-RPC bridge end to end with NO Gemini.
pub(in crate::overlay::computer_control) fn run_mcp_test(
    id: &str,
    tool: Option<&str>,
    args_json: Option<&str>,
    list_only: bool,
) -> Result<(), String> {
    let integration = catalog::get(id).ok_or_else(|| format!("unknown integration '{id}'"))?;
    eprintln!(
        "[mcp-test] spawning {} via '{}'...",
        integration.display_name, integration.launch.program
    );
    let client =
        McpClient::spawn(&integration.launch).map_err(|error| format!("spawn: {error:#}"))?;

    let tools = client
        .list_tools()
        .map_err(|error| format!("tools/list: {error:#}"))?;
    eprintln!("[mcp-test] {} tools:", tools.len());
    for tool in &tools {
        let schema: String = serde_json::to_string(&tool.input_schema)
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect();
        eprintln!(
            "  - {} : {} | schema {schema}",
            tool.name,
            tool.description.chars().take(80).collect::<String>()
        );
    }
    if list_only {
        eprintln!("[mcp-test] list-only; shutting the server down");
        client.shutdown();
        return Ok(());
    }
    if let Some(tool_name) = tool {
        let args = match args_json {
            Some(raw) => serde_json::from_str::<Value>(raw)
                .map_err(|error| format!("bad args JSON: {error}"))?,
            None => json!({}),
        };
        eprintln!("[mcp-test] calling {tool_name} {args}...");
        let result = client
            .call_tool(tool_name, &args)
            .map_err(|error| format!("tool call failed: {error:#}"))?;
        eprintln!(
            "[mcp-test] result: {}",
            serde_json::to_string(&result)
                .unwrap_or_default()
                .chars()
                .take(1000)
                .collect::<String>()
        );
        if !result.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            client.shutdown();
            return Err("explicit tool call returned ok:false".to_string());
        }
    } else {
        let health = support::semantic_health(&client, &tools, integration.semantic_probe_tool);
        eprintln!(
            "[mcp-test] semantic health: {}",
            serde_json::to_string(&health.evidence)
                .unwrap_or_default()
                .chars()
                .take(1200)
                .collect::<String>()
        );
        if !health.ready {
            client.shutdown();
            return Err("semantic health probe failed".to_string());
        }
    }
    eprintln!("[mcp-test] done; shutting the server down");
    client.shutdown();
    Ok(())
}
