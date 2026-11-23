//! Stdio JSON-RPC with one reader and bounded writer per owned MCP server.
//! Requests retain exact reply IDs, write admission, cancellation and deadlines.
//! Interrupted partial frames retire the stream; completed writes may use
//! advisory request cancellation without claiming that an effect was undone.

use super::catalog::LaunchSpec;
use super::client_protocol::{
    ClientLifecycleEvents, ClientLifecycleSignal, lifecycle_channel, reader_loop,
};
#[path = "client_process.rs"]
mod client_process;
#[path = "client_result.rs"]
mod client_result;
#[path = "client_rpc.rs"]
mod client_rpc;
#[path = "client_tools.rs"]
mod client_tools;
#[path = "client_writer.rs"]
mod client_writer;
use crate::process_job::OwnedProcess;
use anyhow::{Result, anyhow, bail};
pub(super) use client_result::format_error;
use client_tools::collect_tool_pages;
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60); // a cold uvx/npx fetch on first run is slow
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
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
    child: Arc<client_process::ServerProcess>,
    writer: client_writer::Writer,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
    alive: Arc<AtomicBool>,
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl McpClient {
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
        if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
            bail!("MCP server launch cancelled");
        }
        let mut command = crate::system_command::background_command(launch.program);
        command
            .args(launch.args)
            .envs(launch.env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()); // never let a chatty server fill + deadlock the pipe
        let mut child = OwnedProcess::spawn(&mut command, ()).map_err(|e| {
            anyhow!(
                "spawn '{}' failed: {e} (is it installed / on PATH?)",
                launch.program
            )
        })?;
        let stdin = child.take_stdin().ok_or_else(|| anyhow!("no stdin pipe"))?;
        let stdout = child
            .take_stdout()
            .ok_or_else(|| anyhow!("no stdout pipe"))?;
        let pending: Arc<Mutex<HashMap<u64, Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
            bail!("MCP server launch cancelled");
        }
        let reader = {
            let pending = pending.clone();
            let alive = alive.clone();
            let lifecycle = lifecycle.clone();
            std::thread::Builder::new()
                .name("sgt-mcp-reader".into())
                .spawn(move || reader_loop(stdout, &pending, &alive, lifecycle.as_ref()))
                .map_err(|error| anyhow!("MCP reader launch failed: {error}"))?
        };
        let child = Arc::new(client_process::ServerProcess::new(
            child,
            alive.clone(),
            pending.clone(),
            lifecycle,
        )?);
        let writer = client_writer::Writer::start(stdin, child.clone(), alive.clone())
            .map_err(|error| anyhow!("MCP writer launch failed: {error}"))?;
        child.resume()?;
        let client = McpClient {
            connection_token,
            child,
            writer,
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
        self.notify_until(
            "notifications/initialized",
            json!({}),
            HANDSHAKE_TIMEOUT,
            stop,
        )
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
    pub fn call_tool_owned(
        &self,
        name: &str,
        args: &Value,
        stop: &Arc<AtomicBool>,
    ) -> Result<Value> {
        self.call_tool_until(name, args, CALL_TIMEOUT, Some(stop))
    }

    pub fn call_tool_timeout(&self, name: &str, args: &Value, timeout: Duration) -> Result<Value> {
        self.call_tool_until(name, args, timeout, None)
    }

    fn call_tool_until(
        &self,
        name: &str,
        args: &Value,
        timeout: Duration,
        stop: Option<&Arc<AtomicBool>>,
    ) -> Result<Value> {
        let params = json!({"name":name,"arguments":args});
        let result = match stop {
            Some(stop) => self.request_owned("tools/call", params, timeout, stop),
            None => self.request_until("tools/call", params, timeout, None),
        }?;
        Ok(client_result::format_result(result))
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn connection_token(&self) -> u64 {
        self.connection_token
    }

    /// Stop the owned tree. Idempotent and bounded; also runs on `Drop`.
    pub fn shutdown(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.child.stop();
        self.writer.join_finished();
        // A stalled pipe must not make connection shutdown unbounded.
        let mut reader = self.reader.lock();
        if reader.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(reader) = reader.take()
        {
            let _ = reader.join();
        }
        self.pending.lock().clear();
    }
}

fn next_connection_token() -> u64 {
    NEXT_CONNECTION_TOKEN.fetch_add(1, Ordering::SeqCst)
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
#[path = "client_native_tests.rs"]
pub(super) mod native_tests;
