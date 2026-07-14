use super::catalog;
use super::client::{McpClient, McpTool};
use super::registry;
use super::schema::{sanitize_schema, unique_decl_name};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// A connected integration: its live client + the tool list it advertised.
struct Connected {
    client: Arc<McpClient>,
    tools: Vec<McpTool>,
}

#[derive(Default)]
struct Manager {
    connected: HashMap<String, Connected>,
    /// declared tool name (`mcp__id__tool`, possibly truncated/de-duped) → (id, real tool name).
    routes: HashMap<String, (String, String)>,
}

/// Per-integration `(id, [(tool name, description, input schema)])` snapshot taken
/// under the lock so declaration-building doesn't borrow `connected` + `routes` at once.
type ConnSnapshot = Vec<(String, Vec<(String, String, Value)>)>;

fn manager() -> &'static parking_lot::Mutex<Manager> {
    static MANAGER: std::sync::OnceLock<parking_lot::Mutex<Manager>> = std::sync::OnceLock::new();
    MANAGER.get_or_init(|| parking_lot::Mutex::new(Manager::default()))
}

/// Set when the connected set changes → the runtime reconnects so `build_setup`
/// re-declares the tools (Gemini Live freezes tools at session setup).
static TOOLS_CHANGED: AtomicBool = AtomicBool::new(false);

pub(in crate::overlay::computer_control) fn tools_changed() -> bool {
    TOOLS_CHANGED.load(Ordering::SeqCst)
}

pub(in crate::overlay::computer_control) fn clear_tools_changed() {
    TOOLS_CHANGED.store(false, Ordering::SeqCst);
}

pub(in crate::overlay::computer_control) fn is_connected(id: &str) -> bool {
    manager()
        .lock()
        .connected
        .get(id)
        .is_some_and(|connection| connection.client.is_alive())
}

pub(super) fn connected_snapshot(id: &str) -> Option<(Arc<McpClient>, Vec<McpTool>)> {
    manager()
        .lock()
        .connected
        .get(id)
        .filter(|connection| connection.client.is_alive())
        .map(|connection| (connection.client.clone(), connection.tools.clone()))
}

/// Spawn + handshake + list tools, store the live client. Idempotent.
pub(super) fn connect(id: &str) -> Result<usize, String> {
    connect_inner(id, None)?.ok_or_else(|| "connection owner stopped".to_string())
}

fn connect_inner(id: &str, stop: Option<&AtomicBool>) -> Result<Option<usize>, String> {
    if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
        return Ok(None);
    }
    if is_connected(id) {
        return Ok(Some(0));
    }
    let integration = catalog::get(id).ok_or_else(|| format!("unknown integration '{id}'"))?;
    let client = match stop {
        Some(stop) => McpClient::spawn_until(&integration.launch, stop),
        None => McpClient::spawn(&integration.launch),
    }
    .map_err(|error| format!("spawn: {error:#}"))?;
    let tools = match stop {
        Some(stop) => client.list_tools_owned(stop),
        None => client.list_tools(),
    }
    .map_err(|error| format!("tools/list: {error:#}"))?;
    let count = tools.len();
    let mut manager = manager().lock();
    let registered = register_if_owner_active(stop, || {
        manager.connected.insert(
            id.to_string(),
            Connected {
                client: Arc::new(client),
                tools,
            },
        );
    });
    drop(manager);
    if !registered {
        return Ok(None);
    }
    TOOLS_CHANGED.store(true, Ordering::SeqCst);
    Ok(Some(count))
}

fn register_if_owner_active(stop: Option<&AtomicBool>, register: impl FnOnce()) -> bool {
    if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
        return false;
    }
    register();
    true
}

pub(super) fn disconnect(id: &str) {
    if let Some(connection) = manager().lock().connected.remove(id) {
        connection.client.shutdown();
    }
    TOOLS_CHANGED.store(true, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupAttempt {
    Connected,
    Failed,
    Stopped,
}

/// Bounded startup barrier for the installed integration catalog. Connection
/// workers remain asynchronous; this handle reports when all attempts settle or
/// when the caller's deadline expires.
pub(in crate::overlay::computer_control) struct StartupCatalog {
    installed: usize,
    attempts: Receiver<StartupAttempt>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct StartupCatalogReport {
    pub installed: usize,
    pub connected: usize,
    pub failed: usize,
    pub pending: usize,
    pub stopped: bool,
}

impl StartupCatalog {
    pub(in crate::overlay::computer_control) fn wait(
        self,
        timeout: Duration,
        stop: &AtomicBool,
    ) -> StartupCatalogReport {
        let deadline = Instant::now() + timeout;
        let mut report = StartupCatalogReport {
            installed: self.installed,
            ..StartupCatalogReport::default()
        };
        while report.connected + report.failed < report.installed {
            if stop.load(Ordering::SeqCst) {
                report.stopped = true;
                break;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let poll = remaining.min(Duration::from_millis(50));
            match self.attempts.recv_timeout(poll) {
                Ok(StartupAttempt::Connected) => report.connected += 1,
                Ok(StartupAttempt::Failed) => report.failed += 1,
                Ok(StartupAttempt::Stopped) => {
                    report.stopped = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    report.failed += report
                        .installed
                        .saturating_sub(report.connected + report.failed);
                    break;
                }
            }
        }
        report.pending = report
            .installed
            .saturating_sub(report.connected + report.failed);
        report
    }
}

/// Bring every installed integration online concurrently and return a bounded
/// lifecycle handle. A slow provider never blocks the owning session thread
/// indefinitely, and a late success still raises `TOOLS_CHANGED` for activation.
pub(in crate::overlay::computer_control) fn connect_all_installed(
    stop: Arc<AtomicBool>,
) -> StartupCatalog {
    let ids = registry::installed_ids();
    let installed = ids.len();
    let (tx, attempts) = mpsc::channel();
    for id in ids {
        let tx = tx.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let outcome = match connect_inner(&id, Some(&stop)) {
                Ok(Some(count)) => {
                    eprintln!("[mcp] connected '{id}' ({count} tools)");
                    StartupAttempt::Connected
                }
                Ok(None) => StartupAttempt::Stopped,
                Err(error) => {
                    if stop.load(Ordering::SeqCst) {
                        StartupAttempt::Stopped
                    } else {
                        eprintln!("[mcp] connect '{id}' failed: {error}");
                        StartupAttempt::Failed
                    }
                }
            };
            let _ = tx.send(outcome);
        });
    }
    drop(tx);
    StartupCatalog {
        installed,
        attempts,
    }
}

/// Kill every server (session stop / app exit) so no child outlives the session.
pub(in crate::overlay::computer_control) fn disconnect_all() {
    let mut manager = manager().lock();
    for (_, connection) in manager.connected.drain() {
        connection.client.shutdown();
    }
    manager.routes.clear();
}

/// Keep the live setup small: connected integrations are discovered and invoked
/// through two stable proxy tools instead of injecting every schema every turn.
/// The legacy route map is still refreshed so an in-flight pre-reconnect call can
/// finish safely.
pub(in crate::overlay::computer_control) fn active_tool_declarations() -> Vec<Value> {
    let mut manager = manager().lock();
    manager.routes.clear();
    // Snapshot to avoid borrowing `connected` and `routes` at once.
    let snapshot: ConnSnapshot = manager
        .connected
        .iter()
        .map(|(id, connection)| {
            (
                id.clone(),
                connection
                    .tools
                    .iter()
                    .map(|tool| {
                        (
                            tool.name.clone(),
                            tool.description.clone(),
                            tool.input_schema.clone(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    if snapshot.is_empty() {
        return Vec::new();
    }
    let mut seen: HashSet<String> = HashSet::new();
    for (id, tools) in &snapshot {
        let display = catalog::get(id)
            .map(|integration| integration.display_name)
            .unwrap_or(id.as_str());
        for (tool_name, description, schema) in tools {
            let name = unique_decl_name(id, tool_name, &mut seen);
            manager.routes.insert(name, (id.clone(), tool_name.clone()));
            let _ = (display, description, schema);
        }
    }
    proxy_tool_declarations()
}

fn proxy_tool_declarations() -> Vec<Value> {
    vec![
        json!({
            "name": "integration_tool_search",
            "description": "Find relevant tools exposed by connected app integrations. Call once with the capability you need; returns a small ranked list with exact integration_id, tool name, description, and input schema.",
            "parameters": {"type": "object", "properties": {
                "query": {"type": "string", "description": "Capability or outcome needed."},
                "integration_id": {"type": "string", "description": "Optional integration id to restrict the search."}
            }, "required": ["query"]}
        }),
        json!({
            "name": "integration_tool_call",
            "description": "Invoke one exact connected integration tool returned by integration_tool_search.",
            "parameters": {"type": "object", "properties": {
                "integration_id": {"type": "string"},
                "tool": {"type": "string"},
                "arguments": {"type": "object", "properties": {}}
            }, "required": ["integration_id", "tool", "arguments"]}
        }),
    ]
}

pub(in crate::overlay::computer_control) fn search_tools(
    query: &str,
    integration_id: Option<&str>,
) -> Value {
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|term| term.len() > 1)
        .map(str::to_string)
        .collect();
    let manager = manager().lock();
    let mut matches: Vec<(usize, Value)> = Vec::new();
    for (id, connected) in &manager.connected {
        if integration_id.is_some_and(|wanted| wanted != id) || !connected.client.is_alive() {
            continue;
        }
        for tool in &connected.tools {
            let haystack = format!("{} {}", tool.name, tool.description).to_lowercase();
            let score = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count();
            if score > 0 || terms.is_empty() {
                matches.push((
                    score,
                    json!({
                        "integration_id": id,
                        "tool": tool.name,
                        "description": tool.description,
                        "parameters": sanitize_schema(&tool.input_schema),
                    }),
                ));
            }
        }
    }
    matches.sort_by(|left, right| right.0.cmp(&left.0));
    let tools = matches
        .into_iter()
        .take(8)
        .map(|(_, tool)| tool)
        .collect::<Vec<_>>();
    json!({"ok": true, "query": query, "tools": tools})
}

pub(in crate::overlay::computer_control) fn call_tool(id: &str, tool: &str, args: &Value) -> Value {
    let Some((client, tools)) = connected_snapshot(id) else {
        return json!({"ok": false, "error": format!("integration '{id}' not connected")});
    };
    if !tools.iter().any(|candidate| candidate.name == tool) {
        return json!({"ok": false, "error": "tool is not exposed by this integration"});
    }
    match client.call_tool(tool, args) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": format!("integration call failed: {error:#}")}),
    }
}

/// Route an `mcp__…` tool call to its server. `None` = not an MCP call (let native
/// dispatch handle it); `Some(err json)` keeps a stale/dead call out of the native
/// "unknown action" path.
pub(in crate::overlay::computer_control) fn try_dispatch(
    name: &str,
    args: &Value,
) -> Option<Value> {
    if !name.starts_with("mcp__") {
        return None;
    }
    let Some((id, tool)) = manager().lock().routes.get(name).cloned() else {
        return Some(json!({"ok": false, "error": "mcp tool not currently available"}));
    };
    let client = manager()
        .lock()
        .connected
        .get(&id)
        .map(|connection| connection.client.clone());
    let Some(client) = client else {
        return Some(json!({"ok": false, "error": format!("integration '{id}' not connected")}));
    };
    Some(match client.call_tool(&tool, args) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": format!("mcp call failed: {error:#}")}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn proxy_catalog_keeps_search_and_call_schemas() {
        let declarations = proxy_tool_declarations();

        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0]["name"], "integration_tool_search");
        assert_eq!(declarations[0]["parameters"]["required"], json!(["query"]));
        assert_eq!(declarations[1]["name"], "integration_tool_call");
        assert_eq!(
            declarations[1]["parameters"]["required"],
            json!(["integration_id", "tool", "arguments"])
        );
    }

    #[test]
    fn non_mcp_names_remain_available_to_native_dispatch() {
        assert_eq!(try_dispatch("future_native_capability", &json!({})), None);
    }

    #[test]
    fn startup_catalog_reports_settled_attempts() {
        let (tx, attempts) = mpsc::channel();
        tx.send(StartupAttempt::Connected).unwrap();
        tx.send(StartupAttempt::Failed).unwrap();
        drop(tx);
        let report = StartupCatalog {
            installed: 2,
            attempts,
        }
        .wait(Duration::from_secs(1), &AtomicBool::new(false));

        assert_eq!(report.connected, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.pending, 0);
        assert!(!report.stopped);
    }

    #[test]
    fn startup_catalog_deadline_is_bounded_and_preserves_pending_count() {
        let (_tx, attempts) = mpsc::channel();
        let report = StartupCatalog {
            installed: 1,
            attempts,
        }
        .wait(Duration::ZERO, &AtomicBool::new(false));

        assert_eq!(report.connected, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.pending, 1);
        assert!(!report.stopped);
    }

    #[test]
    fn stopped_owner_cannot_register_after_attempt_settles() {
        let stop = Arc::new(AtomicBool::new(false));
        let registered = Arc::new(AtomicBool::new(false));
        let (settle_tx, settle_rx) = mpsc::channel();
        let worker_stop = Arc::clone(&stop);
        let worker_registered = Arc::clone(&registered);
        let worker = std::thread::spawn(move || {
            settle_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            register_if_owner_active(Some(&worker_stop), || {
                worker_registered.store(true, Ordering::SeqCst);
            })
        });

        stop.store(true, Ordering::SeqCst);
        settle_tx.send(()).unwrap();
        assert!(!worker.join().unwrap());
        assert!(!registered.load(Ordering::SeqCst));
    }
}
