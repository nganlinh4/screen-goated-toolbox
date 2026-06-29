//! Safe support tools for app-side MCP setup: fetch curated docs and probe
//! declarative readiness. Nothing here executes model-provided commands or URLs.

use serde_json::{Map, Value, json};
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use super::catalog::{self, ReadinessProbe};
use super::client::{McpClient, McpTool};

const DOCS_LIMIT: usize = 24_000;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(12);

pub(super) fn status_tool(
    id: &str,
    connected: Option<(Arc<McpClient>, Vec<McpTool>)>,
    tools_changed: bool,
) -> Value {
    let Some(integ) = catalog::get(id) else {
        return json!({"ok": false, "error": format!("unknown integration '{id}'")});
    };
    let transport = readiness(integ.readiness_probe);
    let server_connected = connected.is_some();
    let health = match connected {
        Some((client, tools)) => semantic_health(&client, &tools),
        None => SemanticHealth {
            ready: false,
            evidence: json!({"kind": "mcp_tool", "error": "MCP server is not connected"}),
        },
    };
    let tools_active = server_connected && !tools_changed;
    let activation_pending = server_connected && health.ready && !tools_active;
    let ready = server_connected && tools_active && health.ready;
    json!({
        "ok": true,
        "id": integ.id,
        "name": integ.display_name,
        "server_connected": server_connected,
        "tools_active": tools_active,
        "activation_pending": activation_pending,
        "app_ready": health.ready,
        "app_probe": health.evidence,
        "transport_probe": transport.evidence,
        "ready": ready,
        "done_when": done_when(integ.id),
        "instruction": if ready {
            "Integration is ready. Stop setup and use its mcp__... tools for the user's task."
        } else if activation_pending {
            "Integration health passed, but its tools are not active in this Live session yet. Stop setup actions; the runtime must reconnect to activate them."
        } else {
            "Not ready yet. Read docs if needed, prefer the app's programmatic/scripting/CLI surface, then re-check this status. Do not loop GUI clicks."
        },
    })
}

pub(super) fn docs_tool(id: &str) -> Value {
    let Some(integ) = catalog::get(id) else {
        return json!({"ok": false, "error": format!("unknown integration '{id}'")});
    };
    match fetch_docs(integ.source_url) {
        Ok(text) => json!({
            "ok": true,
            "id": integ.id,
            "source": integ.source_url,
            "text": text,
            "instruction": "Use these curated-source docs to infer setup. Prefer scripting/CLI/config-file execution over GUI clicking. This is not permission to install anything outside the curated catalog."
        }),
        Err(e) => json!({"ok": false, "source": integ.source_url, "error": e}),
    }
}

pub(super) fn done_when(id: &str) -> String {
    match catalog::get(id).and_then(|i| i.readiness_probe) {
        Some(ReadinessProbe::Tcp { host, port }) => format!(
            "transport probe can reach {host}:{port}, a safe semantic MCP health call succeeds, and tools are active after reconnect"
        ),
        None => "a safe semantic MCP health call succeeds and tools are active after reconnect"
            .to_string(),
    }
}

pub(super) struct SemanticHealth {
    pub ready: bool,
    pub evidence: Value,
}

pub(super) fn semantic_health(client: &McpClient, tools: &[McpTool]) -> SemanticHealth {
    let Some((tool, args)) = select_probe_tool(tools) else {
        return SemanticHealth {
            ready: false,
            evidence: json!({
                "kind": "mcp_tool",
                "error": "no safe read/status/info/list MCP tool with satisfiable required args was found"
            }),
        };
    };
    match client.call_tool_timeout(&tool.name, &args, HEALTH_TIMEOUT) {
        Ok(result) => {
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            SemanticHealth {
                ready: ok,
                evidence: json!({
                    "kind": "mcp_tool",
                    "tool": tool.name,
                    "args": args,
                    "ok": ok,
                    "content": result.get("content").cloned().unwrap_or(Value::Null),
                }),
            }
        }
        Err(e) => SemanticHealth {
            ready: false,
            evidence: json!({
                "kind": "mcp_tool",
                "tool": tool.name,
                "args": args,
                "ok": false,
                "error": format!("{e:#}"),
            }),
        },
    }
}

fn select_probe_tool(tools: &[McpTool]) -> Option<(&McpTool, Value)> {
    let mut candidates: Vec<(u8, &McpTool, Value)> = tools
        .iter()
        .filter_map(|tool| {
            let score = safe_tool_score(tool)?;
            let (args, penalty) = required_args(&tool.input_schema)?;
            Some((score.saturating_add(penalty), tool, args))
        })
        .collect();
    candidates.sort_by_key(|(score, tool, _)| (*score, tool.name.clone()));
    candidates.into_iter().next().map(|(_, tool, args)| (tool, args))
}

fn safe_tool_score(tool: &McpTool) -> Option<u8> {
    let name = tool.name.to_ascii_lowercase();
    let text = format!("{} {}", name, tool.description.to_ascii_lowercase());
    let unsafe_terms = [
        "execute", "download", "import", "generate", "create", "delete", "remove", "update",
        "start", "stop", "poll", "search", "upload", "apply", "save", "write", "edit", "set_",
        "set ",
    ];
    if unsafe_terms.iter().any(|term| text.contains(term)) {
        return None;
    }
    if name.starts_with("get_") && name.contains("info") {
        Some(0)
    } else if name.starts_with("get_") || name.starts_with("read_") {
        Some(1)
    } else if name.contains("status") {
        Some(2)
    } else if name.starts_with("list_") || name.contains("list") {
        Some(3)
    } else if name.contains("info") || name.contains("read") {
        Some(4)
    } else {
        None
    }
}

fn required_args(schema: &Value) -> Option<(Value, u8)> {
    let props = schema.get("properties").and_then(Value::as_object);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut args = Map::new();
    let mut penalty = 0u8;
    for key in required.iter().filter_map(Value::as_str) {
        let field = props.and_then(|p| p.get(key)).unwrap_or(&Value::Null);
        let (value, value_penalty) = probe_value_for(key, field)?;
        penalty = penalty.saturating_add(value_penalty);
        args.insert(key.to_string(), value);
    }
    Some((Value::Object(args), penalty))
}

fn probe_value_for(key: &str, schema: &Value) -> Option<(Value, u8)> {
    let lower = key.to_ascii_lowercase();
    if lower.contains("timezone") || lower == "tz" {
        return Some((json!("UTC"), 0));
    }
    if lower.contains("user_prompt")
        || lower.contains("prompt")
        || lower.contains("question")
        || lower.contains("query")
    {
        return Some((
            json!(
                "Health check: verify the integration can read current state. Do not change anything."
            ),
            0,
        ));
    }
    let ty = schema.get("type").and_then(Value::as_str).unwrap_or("string");
    match ty {
        "string" => Some((json!("health-check"), 8)),
        "integer" => Some((json!(1), 4)),
        "number" => Some((json!(1.0), 4)),
        "boolean" => Some((json!(false), 2)),
        "array" => Some((json!([]), 6)),
        "object" => Some((json!({}), 6)),
        _ => None,
    }
}

struct Readiness {
    evidence: Value,
}

fn readiness(probe: Option<ReadinessProbe>) -> Readiness {
    match probe {
        None => Readiness {
            evidence: json!({"kind": "none", "note": "no app-side readiness probe required"}),
        },
        Some(ReadinessProbe::Tcp { host, port }) => tcp_ready(host, port),
    }
}

fn tcp_ready(host: &str, port: u16) -> Readiness {
    let addr = format!("{host}:{port}");
    let timeout = Duration::from_millis(450);
    let ok = addr
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .is_some_and(|a| TcpStream::connect_timeout(&a, timeout).is_ok());
    Readiness {
        evidence: json!({
            "kind": "tcp",
            "address": addr,
            "connected": ok,
        }),
    }
}

fn fetch_docs(source_url: &str) -> Result<String, String> {
    let readme = github_readme_url(source_url);
    let url = readme.as_deref().unwrap_or(source_url);
    let response = ureq::get(url)
        .header(
            "User-Agent",
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("fetch failed: {e}"))?;
    let mut reader = response
        .into_body()
        .into_reader()
        .take(DOCS_LIMIT as u64 + 1);
    let mut text = String::new();
    reader
        .read_to_string(&mut text)
        .map_err(|e| format!("read failed: {e}"))?;
    if text.len() > DOCS_LIMIT {
        text.truncate(DOCS_LIMIT);
        text.push_str("\n\n[truncated]");
    }
    Ok(text)
}

/// Map ANY `github.com/owner/repo` source to its raw default-branch README (the `HEAD`
/// ref resolves whether the repo uses main or master). Non-GitHub sources are fetched
/// as-is. Derived generically from the catalog `source_url` — no per-app special-casing.
fn github_readme_url(source_url: &str) -> Option<String> {
    let rest = source_url.strip_prefix("https://github.com/")?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    Some(format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn tcp_probe_reports_open_and_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(
            tcp_ready("127.0.0.1", port)
                .evidence
                .get("connected")
                .and_then(Value::as_bool),
            Some(true)
        );
        drop(listener);
        assert_eq!(
            tcp_ready("127.0.0.1", port)
                .evidence
                .get("connected")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn github_readme_url_is_generic() {
        assert_eq!(
            github_readme_url("https://github.com/ahujasid/blender-mcp").as_deref(),
            Some("https://raw.githubusercontent.com/ahujasid/blender-mcp/HEAD/README.md")
        );
        // Works for any repo, not just the seeded ones — no special-casing.
        assert!(github_readme_url("https://github.com/modelcontextprotocol/servers").is_some());
        assert!(github_readme_url("https://example.invalid/anything").is_none());
    }
}
