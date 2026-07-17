//! Safe support tools for app-side MCP setup: fetch curated docs and probe
//! declarative readiness. Nothing here executes model-provided commands or URLs.

use serde_json::{Value, json};
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
        Some((client, tools)) => semantic_health(&client, &tools, integ.semantic_probe_tool),
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
    match fetch_docs(integ.docs_url) {
        Ok(text) => json!({
            "ok": true,
            "id": integ.id,
            "source": integ.source_url,
            "docs_url": integ.docs_url,
            "text": text,
            "instruction": "Use these curated-source docs to infer setup. Prefer scripting/CLI/config-file execution over GUI clicking. This is not permission to install anything outside the curated catalog."
        }),
        Err(e) => json!({
            "ok": false,
            "source": integ.source_url,
            "docs_url": integ.docs_url,
            "error": e,
        }),
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

pub(super) fn semantic_health(
    client: &McpClient,
    tools: &[McpTool],
    catalog_probe_tool: Option<&str>,
) -> SemanticHealth {
    let Some(probe) = select_probe_tool(tools, catalog_probe_tool) else {
        return SemanticHealth {
            ready: false,
            evidence: json!({
                "kind": "mcp_tool",
                "error": "no structurally authorized zero-argument MCP readiness probe was found"
            }),
        };
    };
    let args = json!({});
    match client.call_tool_timeout(&probe.tool.name, &args, HEALTH_TIMEOUT) {
        Ok(result) => {
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            SemanticHealth {
                ready: ok,
                evidence: json!({
                    "kind": "mcp_tool",
                    "tool": probe.tool.name,
                    "args": args,
                    "authorization": probe.authorization.as_str(),
                    "ok": ok,
                    "content": result.get("content").cloned().unwrap_or(Value::Null),
                }),
            }
        }
        Err(e) => SemanticHealth {
            ready: false,
            evidence: json!({
                "kind": "mcp_tool",
                "tool": probe.tool.name,
                "args": args,
                "authorization": probe.authorization.as_str(),
                "ok": false,
                "error": format!("{e:#}"),
            }),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeAuthorization {
    ReadOnlyClosedWorld,
    ReadOnlyOpenWorld,
    Catalog,
}

impl ProbeAuthorization {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlyClosedWorld => "mcp_read_only_closed_world",
            Self::ReadOnlyOpenWorld => "mcp_read_only_open_world",
            Self::Catalog => "curated_catalog",
        }
    }
}

struct ProbeSelection<'a> {
    tool: &'a McpTool,
    authorization: ProbeAuthorization,
}

fn select_probe_tool<'a>(
    tools: &'a [McpTool],
    catalog_probe_tool: Option<&str>,
) -> Option<ProbeSelection<'a>> {
    let mut candidates = tools
        .iter()
        .filter(|tool| schema_accepts_empty_object(&tool.input_schema))
        .filter_map(|tool| {
            let authorization = probe_authorization(tool, catalog_probe_tool)?;
            Some((authorization_rank(authorization), tool, authorization))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (left.0, left.1.name.as_str()).cmp(&(right.0, right.1.name.as_str()))
    });
    candidates
        .into_iter()
        .next()
        .map(|(_, tool, authorization)| ProbeSelection {
            tool,
            authorization,
        })
}

fn probe_authorization(
    tool: &McpTool,
    catalog_probe_tool: Option<&str>,
) -> Option<ProbeAuthorization> {
    if tool.annotations.destructive == Some(true) {
        return None;
    }
    match tool.annotations.read_only {
        Some(true) => match tool.annotations.open_world {
            Some(false) => Some(ProbeAuthorization::ReadOnlyClosedWorld),
            _ => Some(ProbeAuthorization::ReadOnlyOpenWorld),
        },
        Some(false) => None,
        None if catalog_probe_tool == Some(tool.name.as_str()) => Some(ProbeAuthorization::Catalog),
        None => None,
    }
}

fn authorization_rank(authorization: ProbeAuthorization) -> u8 {
    match authorization {
        ProbeAuthorization::ReadOnlyClosedWorld => 0,
        ProbeAuthorization::ReadOnlyOpenWorld => 1,
        ProbeAuthorization::Catalog => 2,
    }
}

fn schema_accepts_empty_object(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return false;
    }
    let required_is_empty = match object.get("required") {
        None => true,
        Some(Value::Array(required)) => required.is_empty(),
        Some(_) => false,
    };
    if !required_is_empty {
        return false;
    }
    match object.get("minProperties") {
        None => {}
        Some(Value::Number(value)) if value.as_u64() == Some(0) => {}
        Some(_) => return false,
    }
    ![
        "$ref", "allOf", "anyOf", "oneOf", "not", "if", "then", "else", "const", "enum",
    ]
    .iter()
    .any(|keyword| object.contains_key(*keyword))
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

fn fetch_docs(docs_url: &str) -> Result<String, String> {
    let url = validated_docs_url(docs_url)?;
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

fn validated_docs_url(docs_url: &str) -> Result<&str, String> {
    let parsed = url::Url::parse(docs_url).map_err(|e| format!("invalid docs URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(docs_url),
        scheme => Err(format!("unsupported docs URL scheme '{scheme}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::client::McpToolAnnotations;
    use super::*;
    use std::net::TcpListener;

    fn tool(name: &str, schema: Value, annotations: McpToolAnnotations) -> McpTool {
        McpTool {
            name: name.to_string(),
            description: "arbitrary catalog prose".to_string(),
            input_schema: schema,
            annotations,
        }
    }

    fn read_only(open_world: bool) -> McpToolAnnotations {
        McpToolAnnotations {
            read_only: Some(true),
            destructive: Some(false),
            open_world: Some(open_world),
        }
    }

    #[test]
    fn probe_prefers_structural_closed_world_annotation() {
        let tools = vec![
            tool("catalog", json!({"type": "object"}), Default::default()),
            tool("external", json!({"type": "object"}), read_only(true)),
            tool("local", json!({"type": "object"}), read_only(false)),
        ];

        let selected = select_probe_tool(&tools, Some("catalog")).unwrap();

        assert_eq!(selected.tool.name, "local");
        assert_eq!(
            selected.authorization,
            ProbeAuthorization::ReadOnlyClosedWorld
        );
    }

    #[test]
    fn catalog_fallback_is_exact_and_zero_argument_only() {
        let tools = vec![
            tool("other", json!({"type": "object"}), Default::default()),
            tool("approved", json!({"type": "object"}), Default::default()),
        ];
        let selected = select_probe_tool(&tools, Some("approved")).unwrap();
        assert_eq!(selected.tool.name, "approved");
        assert_eq!(selected.authorization, ProbeAuthorization::Catalog);

        let required = vec![tool(
            "approved",
            json!({"type": "object", "required": ["value"]}),
            Default::default(),
        )];
        assert!(select_probe_tool(&required, Some("approved")).is_none());
    }

    #[test]
    fn absent_or_unsafe_metadata_fails_closed() {
        let unsafe_tools = vec![
            tool("unknown", json!({"type": "object"}), Default::default()),
            tool(
                "mutable",
                json!({"type": "object"}),
                McpToolAnnotations {
                    read_only: Some(false),
                    destructive: Some(false),
                    open_world: Some(false),
                },
            ),
            tool(
                "contradictory",
                json!({"type": "object"}),
                McpToolAnnotations {
                    read_only: Some(true),
                    destructive: Some(true),
                    open_world: Some(false),
                },
            ),
        ];

        assert!(select_probe_tool(&unsafe_tools, None).is_none());
        assert!(select_probe_tool(&unsafe_tools, Some("mutable")).is_none());
        assert!(select_probe_tool(&unsafe_tools, Some("contradictory")).is_none());
    }

    #[test]
    fn malformed_input_schema_cannot_be_probed() {
        for schema in [
            json!(null),
            json!({}),
            json!({"type": "array"}),
            json!({"type": "object", "required": "value"}),
            json!({"type": "object", "minProperties": 1}),
            json!({"type": "object", "allOf": [{"required": ["value"]}]}),
        ] {
            let tools = vec![tool("candidate", schema, read_only(false))];
            assert!(select_probe_tool(&tools, None).is_none());
        }
    }

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
    fn explicit_docs_url_is_used_without_host_or_path_rewriting() {
        let source = "https://docs.vendor.invalid/repos/widget?channel=stable#setup";
        assert_eq!(
            validated_docs_url(source).unwrap(),
            source,
            "catalog metadata is authoritative"
        );
    }

    #[test]
    fn malformed_or_non_http_docs_metadata_fails_structurally() {
        assert!(validated_docs_url("").is_err());
        assert!(validated_docs_url("relative/docs").is_err());
        assert!(validated_docs_url("file:///private/docs").is_err());
    }
}
