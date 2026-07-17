//! Minimal SYNC stdio JSON-RPC client for ONE MCP server (the codebase has no
//! tokio). Shape: one reader thread + an id-keyed pending map; `request()` writes a
//! single newline-delimited JSON line and blocks on a per-request channel until the
//! reader routes the matching response back. The child is killed + joined on
//! `shutdown`/`Drop`, and dropping its stdin signals the server to exit cleanly.

use super::catalog::LaunchSpec;
use super::client_protocol::{
    ClientLifecycleEvents, ClientLifecycleSignal, lifecycle_channel, reader_loop,
};
use super::schema::bounded_prose;
use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60); // a cold uvx/npx fetch on first run is slow
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_TOOL_LIST_PAGES: usize = 1024;
static NEXT_CONNECTION_TOKEN: AtomicU64 = AtomicU64::new(1);

/// One tool exposed by an MCP server.
#[derive(Clone)]
pub(super) struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub annotations: McpToolAnnotations,
}

/// Structured MCP risk hints. Servers are pinned by the curated catalog, but
/// absent or malformed hints still retain the protocol's conservative meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct McpToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub open_world: Option<bool>,
}

/// A live connection to one stdio MCP server.
pub(super) struct McpClient {
    connection_token: u64,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
    alive: Arc<AtomicBool>,
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl McpClient {
    /// Spawn the server, complete the MCP handshake, and return a ready client.
    pub fn spawn(launch: &LaunchSpec) -> Result<McpClient> {
        Self::spawn_inner(launch, None, next_connection_token(), None)
    }

    pub fn spawn_managed(
        launch: &LaunchSpec,
        stop: Option<&AtomicBool>,
    ) -> Result<(McpClient, ClientLifecycleEvents)> {
        let connection_token = next_connection_token();
        let (signal, events) = lifecycle_channel(connection_token);
        let client = Self::spawn_inner(launch, stop, connection_token, Some(signal))?;
        Ok((client, events))
    }

    fn spawn_inner(
        launch: &LaunchSpec,
        stop: Option<&AtomicBool>,
        connection_token: u64,
        lifecycle: Option<ClientLifecycleSignal>,
    ) -> Result<McpClient> {
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new(launch.program)
            .args(launch.args)
            .envs(launch.env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // never let a chatty server fill + deadlock the pipe
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| {
                anyhow!(
                    "spawn '{}' failed: {e} (is it installed / on PATH?)",
                    launch.program
                )
            })?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("no stdout pipe"))?;
        let pending: Arc<Mutex<HashMap<u64, Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let reader = {
            let pending = pending.clone();
            let alive = alive.clone();
            std::thread::spawn(move || reader_loop(stdout, &pending, &alive, lifecycle.as_ref()))
        };
        let client = McpClient {
            connection_token,
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            pending,
            alive,
            reader: Mutex::new(Some(reader)),
        };
        client.handshake(stop)?;
        Ok(client)
    }

    fn handshake(&self, stop: Option<&AtomicBool>) -> Result<()> {
        self.request_until(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "screen-goated-toolbox", "version": env!("CARGO_PKG_VERSION")}
            }),
            HANDSHAKE_TIMEOUT,
            stop,
        )?;
        self.notify("notifications/initialized", json!({}))
    }

    pub fn list_tools(&self) -> Result<Vec<McpTool>> {
        self.list_tools_until(None)
    }

    pub fn list_tools_owned(&self, stop: &AtomicBool) -> Result<Vec<McpTool>> {
        self.list_tools_until(Some(stop))
    }

    fn list_tools_until(&self, stop: Option<&AtomicBool>) -> Result<Vec<McpTool>> {
        let deadline = Instant::now() + LIST_TIMEOUT;
        collect_tool_pages(|params| {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("mcp 'tools/list' timed out");
            }
            self.request_until("tools/list", params, remaining, stop)
        })
    }

    /// Call a tool. Returns `{ok, content (joined text), raw}` so the model gets a
    /// readable result regardless of the server's content shape.
    pub fn call_tool(&self, name: &str, args: &Value) -> Result<Value> {
        self.call_tool_timeout(name, args, CALL_TIMEOUT)
    }

    pub fn call_tool_timeout(&self, name: &str, args: &Value, timeout: Duration) -> Result<Value> {
        let result = self.request(
            "tools/call",
            json!({"name": name, "arguments": args}),
            timeout,
        )?;
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(json!({"ok": !is_error, "content": content_text(&result), "raw": result}))
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn connection_token(&self) -> u64 {
        self.connection_token
    }

    /// Kill the child. Idempotent; also runs on `Drop`.
    pub fn shutdown(&self) {
        self.alive.store(false, Ordering::SeqCst);
        {
            let mut child = self.child.lock();
            kill_process_tree(child.id());
            let _ = child.kill();
            for _ in 0..10 {
                match child.try_wait() {
                    Ok(Some(_)) | Err(_) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        }
        // Do not block on the reader thread. Some stdio servers/Windows pipe
        // combinations leave read_line parked briefly even after child kill, and
        // shutdown must never hang the CLI smoke path or app stop path.
        let _ = self.reader.lock().take();
        self.pending.lock().clear();
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.request_until(method, params, timeout, None)
    }

    fn request_until(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        stop: Option<&AtomicBool>,
    ) -> Result<Value> {
        if !self.alive.load(Ordering::SeqCst) {
            bail!("mcp server is not running");
        }
        if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
            bail!("mcp request cancelled because its session stopped");
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().insert(id, tx);
        if let Err(e) = self
            .write_line(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        {
            self.pending.lock().remove(&id);
            bail!("mcp write failed: {e}");
        }
        let deadline = Instant::now() + timeout;
        loop {
            if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
                self.pending.lock().remove(&id);
                bail!("mcp request cancelled because its session stopped");
            }
            let now = Instant::now();
            if now >= deadline {
                self.pending.lock().remove(&id);
                bail!("mcp '{method}' timed out");
            }
            let poll = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(100));
            match rx.recv_timeout(poll) {
                Ok(resp) => {
                    if let Some(err) = resp.get("error") {
                        bail!(
                            "mcp error: {}",
                            err.get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                        );
                    }
                    return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.pending.lock().remove(&id);
                    bail!("mcp server disconnected while handling '{method}'");
                }
            }
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write_line(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .map_err(|e| anyhow!("mcp notify failed: {e}"))
    }

    fn write_line(&self, msg: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(msg).unwrap_or_default();
        line.push(b'\n');
        let mut stdin = self.stdin.lock();
        stdin.write_all(&line)?;
        stdin.flush()
    }
}

fn next_connection_token() -> u64 {
    NEXT_CONNECTION_TOKEN.fetch_add(1, Ordering::SeqCst)
}

fn collect_tool_pages(
    mut request_page: impl FnMut(Value) -> Result<Value>,
) -> Result<Vec<McpTool>> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    let mut seen_tools = HashSet::new();

    for _ in 0..MAX_TOOL_LIST_PAGES {
        let params = cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
        let result = request_page(params)?;
        let page_tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("mcp tools/list page is missing its tools array"))?;
        for (index, tool) in page_tools.iter().enumerate() {
            let parsed = parse_tool(tool)
                .map_err(|error| anyhow!("mcp tools/list tool {index} is malformed: {error}"))?;
            if !seen_tools.insert(parsed.name.clone()) {
                bail!(
                    "mcp tools/list returned duplicate tool name '{}'",
                    parsed.name
                );
            }
            tools.push(parsed);
        }

        let Some(next_cursor) = result.get("nextCursor") else {
            return Ok(tools);
        };
        if next_cursor.is_null() {
            return Ok(tools);
        }
        let next_cursor = next_cursor
            .as_str()
            .ok_or_else(|| anyhow!("mcp tools/list returned a non-string nextCursor"))?
            .to_string();
        if !seen_cursors.insert(next_cursor.clone()) {
            bail!("mcp tools/list cursor cycle detected");
        }
        cursor = Some(next_cursor);
    }

    bail!("mcp tools/list exceeded the bounded pagination limit of {MAX_TOOL_LIST_PAGES} pages")
}

fn parse_tool(value: &Value) -> Result<McpTool> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("tool entry is not an object"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("tool name is missing or empty"))?;
    let description = match object.get("description") {
        Some(description) => description
            .as_str()
            .ok_or_else(|| anyhow!("tool description is not a string"))?,
        None => "",
    };
    let input_schema = object
        .get("inputSchema")
        .filter(|schema| schema.is_object())
        .cloned()
        .ok_or_else(|| anyhow!("tool inputSchema is missing or not an object"))?;
    if object
        .get("annotations")
        .is_some_and(|annotations| !annotations.is_object())
    {
        bail!("tool annotations is not an object");
    }
    Ok(McpTool {
        name: name.to_string(),
        description: bounded_prose(description),
        input_schema,
        annotations: parse_annotations(object.get("annotations")),
    })
}

fn parse_annotations(value: Option<&Value>) -> McpToolAnnotations {
    let boolean = |key| value?.get(key)?.as_bool();
    McpToolAnnotations {
        read_only: boolean("readOnlyHint"),
        destructive: boolean("destructiveHint"),
        open_world: boolean("openWorldHint"),
    }
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_process_tree(_pid: u32) {}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Join the text parts of a `tools/call` result's `content` array (clipped).
fn content_text(result: &Value) -> String {
    let Some(items) = result.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    let mut out = String::new();
    for it in items {
        if let Some(t) = it.get("text").and_then(Value::as_str) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t);
        }
    }
    out.chars().take(4000).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn parses_only_boolean_tool_annotations() {
        assert_eq!(
            parse_annotations(Some(&json!({
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": "false"
            }))),
            McpToolAnnotations {
                read_only: Some(true),
                destructive: Some(false),
                open_world: None,
            }
        );
        assert_eq!(parse_annotations(None), McpToolAnnotations::default());
    }

    #[test]
    fn tool_listing_follows_every_cursor_and_preserves_page_order() {
        let mut pages = VecDeque::from([
            json!({
                "tools": [{"name": "first", "description": "one", "inputSchema": {"type": "object"}}],
                "nextCursor": "page-2"
            }),
            json!({
                "tools": [{"name": "second", "description": "two", "inputSchema": {}}],
                "nextCursor": "page-3"
            }),
            json!({"tools": [{"name": "third", "description": "three", "inputSchema": {}}]}),
        ]);
        let mut params = Vec::new();

        let tools = collect_tool_pages(|request| {
            params.push(request);
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        })
        .expect("all pages should be collected");

        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(
            params,
            [
                json!({}),
                json!({"cursor": "page-2"}),
                json!({"cursor": "page-3"})
            ]
        );
        assert!(pages.is_empty());
    }

    #[test]
    fn tool_listing_treats_null_cursor_as_completion() {
        let mut calls = 0;
        let tools = collect_tool_pages(|_| {
            calls += 1;
            Ok(json!({"tools": [{"name": "only", "inputSchema": {}}], "nextCursor": null}))
        })
        .expect("null cursor should end pagination");

        assert_eq!(calls, 1);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn tool_listing_rejects_cursor_cycles() {
        let mut pages = VecDeque::from([
            json!({"tools": [], "nextCursor": "again"}),
            json!({"tools": [], "nextCursor": "again"}),
        ]);

        let result = collect_tool_pages(|_| {
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        });
        let error = result.err().expect("repeated cursor must not loop forever");

        assert!(error.to_string().contains("cursor cycle"));
    }

    #[test]
    fn tool_listing_rejects_malformed_cursor() {
        let error = collect_tool_pages(|_| Ok(json!({"tools": [], "nextCursor": 7})))
            .err()
            .expect("cursor must be an opaque string");

        assert!(error.to_string().contains("non-string nextCursor"));
    }

    #[test]
    fn tool_listing_requires_an_array_on_every_page() {
        let error = collect_tool_pages(|_| Ok(json!({"tools": {}})))
            .err()
            .expect("a malformed page must invalidate the complete catalog");

        assert!(error.to_string().contains("tools array"));
    }

    #[test]
    fn tool_listing_rejects_a_malformed_entry_instead_of_returning_a_partial_catalog() {
        let error = collect_tool_pages(|_| {
            Ok(json!({
                "tools": [
                    {"name": "valid", "inputSchema": {}},
                    {"name": "missing-schema"}
                ]
            }))
        })
        .err()
        .expect("one malformed entry must invalidate the complete catalog");

        assert!(error.to_string().contains("tool 1 is malformed"));
        assert!(error.to_string().contains("inputSchema"));
    }

    #[test]
    fn tool_listing_rejects_duplicate_names_across_pages() {
        let mut pages = VecDeque::from([
            json!({"tools": [{"name": "same", "inputSchema": {}}], "nextCursor": "next"}),
            json!({"tools": [{"name": "same", "inputSchema": {}}]}),
        ]);
        let error = collect_tool_pages(|_| {
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        })
        .err()
        .expect("duplicate routes must invalidate the complete catalog");

        assert!(error.to_string().contains("duplicate tool name"));
    }

    #[test]
    fn tool_listing_preserves_an_opaque_valid_input_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "future": {
                    "type": ["string", "null"],
                    "vendorExtension": {"nested": [1, true, null]}
                }
            },
            "unknownTopLevelKeyword": "keep-me"
        });
        let tools = collect_tool_pages(|_| {
            Ok(json!({"tools": [{"name": "future", "inputSchema": schema.clone()}]}))
        })
        .expect("opaque object schemas are valid transport data");

        assert_eq!(tools[0].input_schema, schema);
    }
}
