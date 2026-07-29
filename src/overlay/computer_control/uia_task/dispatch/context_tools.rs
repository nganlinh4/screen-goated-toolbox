//! Saved-conversation and optional-integration dispatch.

use super::super::*;

impl Brain {
    pub(super) fn dispatch_context_tool(&self, name: &str, args: &Value) -> Option<Value> {
        Some(match name {
            "search_memory" => {
                let query = args.get("query").and_then(Value::as_str).unwrap_or("");
                let hits = super::super::super::memory::search(query, 5);
                if hits.is_empty() {
                    json!({"ok": true, "results": [], "note": "no matching past conversation"})
                } else {
                    let results: Vec<Value> = hits
                        .iter()
                        .map(|hit| {
                            json!({
                                "id": hit.id.to_string(),
                                "when": hit.timestamp,
                                "title": hit.title,
                                "snippet": hit.snippet,
                            })
                        })
                        .collect();
                    json!({
                        "ok": true,
                        "results": results,
                        "instruction": "Results are ranked by relevance + recency; each has a 'when' timestamp. For 'the last/most recent/previous conversation', pick the one with the newest 'when'. Then open_memory(id) to read it in full.",
                    })
                }
            }
            "open_memory" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                match id
                    .parse::<i64>()
                    .ok()
                    .and_then(super::super::super::memory::open)
                {
                    Some(transcript) => json!({"ok": true, "transcript": transcript}),
                    None => json!({"ok": false, "error": "no saved conversation with that id"}),
                }
            }
            "list_app_integrations" => super::super::super::mcp::list_tool(),
            "setup_app_integration" => super::super::super::mcp::setup_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
                args.get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            "app_integration_status" => super::super::super::mcp::status_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
            ),
            "read_app_integration_docs" => super::super::super::mcp::docs_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
            ),
            "remove_app_integration" => super::super::super::mcp::remove_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
            ),
            _ => return None,
        })
    }
}
