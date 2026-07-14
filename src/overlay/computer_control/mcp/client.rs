//! Minimal SYNC stdio JSON-RPC client for ONE MCP server (the codebase has no
//! tokio). Shape: one reader thread + an id-keyed pending map; `request()` writes a
//! single newline-delimited JSON line and blocks on a per-request channel until the
//! reader routes the matching response back. The child is killed + joined on
//! `shutdown`/`Drop`, and dropping its stdin signals the server to exit cleanly.

use super::catalog::LaunchSpec;
use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60); // a cold uvx/npx fetch on first run is slow
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

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
        Self::spawn_inner(launch, None)
    }

    /// Session-owned startup variant. A stopped owner aborts bounded protocol
    /// waits; dropping the partial client then shuts down its child process.
    pub fn spawn_until(launch: &LaunchSpec, stop: &AtomicBool) -> Result<McpClient> {
        Self::spawn_inner(launch, Some(stop))
    }

    fn spawn_inner(launch: &LaunchSpec, stop: Option<&AtomicBool>) -> Result<McpClient> {
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
            std::thread::spawn(move || reader_loop(stdout, &pending, &alive))
        };
        let client = McpClient {
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
        let result = self.request_until("tools/list", json!({}), LIST_TIMEOUT, stop)?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(tools
            .iter()
            .filter_map(|t| {
                Some(McpTool {
                    name: t.get("name").and_then(Value::as_str)?.to_string(),
                    description: t
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    input_schema: t.get("inputSchema").cloned().unwrap_or_else(|| json!({})),
                    annotations: parse_annotations(t.get("annotations")),
                })
            })
            .collect())
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

/// Reader thread: route each id-bearing response to its waiting request, ignore
/// notifications. On EOF/error mark dead and drop all pending senders (so blocked
/// callers wake with a disconnect error rather than hanging to their timeout).
fn reader_loop(
    stdout: ChildStdout,
    pending: &Mutex<HashMap<u64, Sender<Value>>>,
    alive: &AtomicBool,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(trimmed)
                    && let Some(id) = v.get("id").and_then(Value::as_u64)
                    && let Some(tx) = pending.lock().remove(&id)
                {
                    let _ = tx.send(v);
                }
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
    pending.lock().clear();
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
}
