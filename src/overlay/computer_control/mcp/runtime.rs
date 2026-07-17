use super::catalog;
use super::client::{McpClient, McpTool};
use super::client_protocol::{ClientLifecycleEvents, ClientLifecycleKind};
use super::registry;
use super::schema::{bounded_prose, sanitize_schema, unique_decl_name};
use super::startup::{StartupAttempt, StartupCatalog};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

/// A connected integration: its live client + the tool list it advertised.
struct Connected {
    client: Arc<McpClient>,
    tools: Vec<McpTool>,
    catalog_valid: bool,
}

#[derive(Default)]
struct Manager {
    connected: HashMap<String, Connected>,
    /// Declared name → exact dispatch route plus protocol-authored effect metadata.
    routes: HashMap<String, ToolRoute>,
}

#[derive(Clone)]
struct ToolSnapshot {
    name: String,
    description: String,
    input_schema: Value,
    annotations: super::client::McpToolAnnotations,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolRoute {
    integration_id: String,
    tool_name: String,
    annotations: super::client::McpToolAnnotations,
    connection_token: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConnectionStatus {
    connection_token: u64,
    catalog_valid: bool,
    alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefreshDisposition {
    Replace,
    Remove,
    Ignore,
}

fn refresh_disposition(
    current: Option<ConnectionStatus>,
    connection_token: u64,
    refresh_succeeded: bool,
) -> RefreshDisposition {
    let Some(current) = current else {
        return RefreshDisposition::Ignore;
    };
    if current.connection_token != connection_token {
        return RefreshDisposition::Ignore;
    }
    if refresh_succeeded && current.alive {
        RefreshDisposition::Replace
    } else {
        RefreshDisposition::Remove
    }
}

fn connection_status(connection: &Connected) -> ConnectionStatus {
    ConnectionStatus {
        connection_token: connection.client.connection_token(),
        catalog_valid: connection.catalog_valid,
        alive: connection.client.is_alive(),
    }
}

/// Per-integration snapshot taken under the lock so declaration-building does
/// not borrow `connected` and `routes` at once.
type ConnSnapshot = Vec<(String, u64, Vec<ToolSnapshot>)>;

fn manager() -> &'static parking_lot::Mutex<Manager> {
    static MANAGER: std::sync::OnceLock<parking_lot::Mutex<Manager>> = std::sync::OnceLock::new();
    MANAGER.get_or_init(|| parking_lot::Mutex::new(Manager::default()))
}

/// Monotonic catalog generation. A boolean loses an event when a concurrent
/// lifecycle update lands between a reconnect check and its clear operation.
struct CatalogChangeClock {
    generation: AtomicU64,
    consumed: AtomicU64,
}

impl CatalogChangeClock {
    const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
        }
    }

    fn mark(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn changed(&self) -> bool {
        self.generation.load(Ordering::SeqCst) != self.consumed.load(Ordering::SeqCst)
    }

    fn clear(&self) {
        self.consumed
            .store(self.generation.load(Ordering::SeqCst), Ordering::SeqCst);
    }
}

static CATALOG_CHANGES: CatalogChangeClock = CatalogChangeClock::new();

fn mark_catalog_changed() {
    CATALOG_CHANGES.mark();
}

pub(in crate::overlay::computer_control) fn tools_changed() -> bool {
    CATALOG_CHANGES.changed()
}

pub(in crate::overlay::computer_control) fn clear_tools_changed() {
    CATALOG_CHANGES.clear();
}

pub(in crate::overlay::computer_control) fn is_connected(id: &str) -> bool {
    manager()
        .lock()
        .connected
        .get(id)
        .is_some_and(|connection| connection.catalog_valid && connection.client.is_alive())
}

fn has_live_connection(id: &str) -> bool {
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
        .filter(|connection| connection.catalog_valid && connection.client.is_alive())
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
    if has_live_connection(id) {
        return Ok(Some(0));
    }
    let integration = catalog::get(id).ok_or_else(|| format!("unknown integration '{id}'"))?;
    let (client, lifecycle) = McpClient::spawn_managed(&integration.launch, stop)
        .map_err(|error| format!("spawn: {error:#}"))?;
    let tools = match stop {
        Some(stop) => client.list_tools_owned(stop),
        None => client.list_tools(),
    }
    .map_err(|error| format!("tools/list: {error:#}"))?;
    let count = tools.len();
    let connection_token = client.connection_token();
    let client = Arc::new(client);
    let mut manager = manager().lock();
    let registered = register_if_owner_active(stop, || {
        manager.connected.insert(
            id.to_string(),
            Connected {
                client: Arc::clone(&client),
                tools,
                catalog_valid: true,
            },
        );
    });
    drop(manager);
    if !registered {
        return Ok(None);
    }
    spawn_lifecycle_worker(id.to_string(), connection_token, lifecycle);
    mark_catalog_changed();
    Ok(Some(count))
}

fn register_if_owner_active(stop: Option<&AtomicBool>, register: impl FnOnce()) -> bool {
    if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
        return false;
    }
    register();
    true
}

fn spawn_lifecycle_worker(
    integration_id: String,
    connection_token: u64,
    lifecycle: ClientLifecycleEvents,
) {
    std::thread::spawn(move || {
        consume_lifecycle_events(lifecycle, connection_token, |kind| match kind {
            ClientLifecycleKind::ToolsChanged => refresh_catalog(&integration_id, connection_token),
            ClientLifecycleKind::Disconnected => {
                remove_current_connection(&integration_id, connection_token);
            }
        });
    });
}

fn consume_lifecycle_events(
    lifecycle: ClientLifecycleEvents,
    connection_token: u64,
    mut handle: impl FnMut(ClientLifecycleKind),
) {
    if lifecycle.connection_token() != connection_token {
        return;
    }
    while let Some(batch) = lifecycle.recv() {
        if batch.disconnected {
            handle(ClientLifecycleKind::Disconnected);
            return;
        }
        if batch.tools_changed {
            handle(ClientLifecycleKind::ToolsChanged);
        }
    }
}

/// Invalidate under the manager lock before performing the blocking relist on
/// this worker. This makes old annotations and dispatch routes unavailable at
/// the notification boundary, not after network/process work completes.
fn refresh_catalog(integration_id: &str, connection_token: u64) {
    let client = {
        let mut manager = manager().lock();
        let Some(connection) = manager.connected.get_mut(integration_id) else {
            return;
        };
        if connection.client.connection_token() != connection_token {
            return;
        }
        connection.catalog_valid = false;
        connection.tools.clear();
        let client = Arc::clone(&connection.client);
        manager
            .routes
            .retain(|_, route| route.connection_token != connection_token);
        client
    };
    mark_catalog_changed();

    match client.list_tools() {
        Ok(tools) => replace_current_catalog(integration_id, connection_token, tools),
        Err(error) => {
            eprintln!("[mcp] refresh '{integration_id}' failed: {error:#}");
            remove_current_connection(integration_id, connection_token);
            client.shutdown();
        }
    }
}

fn replace_current_catalog(integration_id: &str, connection_token: u64, tools: Vec<McpTool>) {
    let replaced = {
        let mut manager = manager().lock();
        let Some(connection) = manager.connected.get_mut(integration_id) else {
            return;
        };
        if refresh_disposition(Some(connection_status(connection)), connection_token, true)
            != RefreshDisposition::Replace
        {
            return;
        }
        connection.tools = tools;
        connection.catalog_valid = true;
        true
    };
    if replaced {
        mark_catalog_changed();
    }
}

fn remove_current_connection(integration_id: &str, connection_token: u64) {
    let removed = {
        let mut manager = manager().lock();
        let disposition = manager
            .connected
            .get(integration_id)
            .map(connection_status)
            .map_or(RefreshDisposition::Ignore, |current| {
                refresh_disposition(Some(current), connection_token, false)
            });
        if disposition != RefreshDisposition::Remove {
            return;
        }
        manager.connected.remove(integration_id);
        manager
            .routes
            .retain(|_, route| route.connection_token != connection_token);
        true
    };
    if removed {
        mark_catalog_changed();
    }
}

pub(super) fn disconnect(id: &str) {
    let connection = {
        let mut manager = manager().lock();
        let connection = manager.connected.remove(id);
        if let Some(connection) = &connection {
            manager
                .routes
                .retain(|_, route| route.connection_token != connection.client.connection_token());
        }
        connection
    };
    if let Some(connection) = connection {
        connection.client.shutdown();
    }
    mark_catalog_changed();
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
    StartupCatalog::new(installed, attempts)
}

/// Kill every server (session stop / app exit) so no child outlives the session.
pub(in crate::overlay::computer_control) fn disconnect_all() {
    let mut manager = manager().lock();
    let changed = !manager.connected.is_empty() || !manager.routes.is_empty();
    for (_, connection) in manager.connected.drain() {
        connection.client.shutdown();
    }
    manager.routes.clear();
    drop(manager);
    if changed {
        mark_catalog_changed();
    }
}

/// Declare every tool exposed by every live connection. The model receives the
/// complete capability catalog and owns semantic selection; Rust only namespaces
/// names, sanitizes wire schemas, and records exact dispatch routes.
pub(in crate::overlay::computer_control) fn active_tool_declarations() -> Vec<Value> {
    let mut manager = manager().lock();
    manager.routes.clear();
    // Snapshot to avoid borrowing `connected` and `routes` at once.
    let snapshot: ConnSnapshot = manager
        .connected
        .iter()
        .filter(|(_, connection)| connection.catalog_valid && connection.client.is_alive())
        .map(|(id, connection)| {
            (
                id.clone(),
                connection.client.connection_token(),
                connection
                    .tools
                    .iter()
                    .map(|tool| ToolSnapshot {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema: tool.input_schema.clone(),
                        annotations: tool.annotations,
                    })
                    .collect(),
            )
        })
        .collect();
    let (declarations, routes) = direct_declarations(&snapshot);
    manager.routes = routes;
    declarations
}

fn direct_declarations(snapshot: &ConnSnapshot) -> (Vec<Value>, HashMap<String, ToolRoute>) {
    let tool_count = snapshot.iter().map(|(_, _, tools)| tools.len()).sum();
    let mut declarations = Vec::with_capacity(tool_count);
    let mut routes = HashMap::with_capacity(tool_count);
    let mut seen: HashSet<String> = HashSet::new();
    for (id, connection_token, tools) in snapshot {
        let display = catalog::get(id)
            .map(|integration| integration.display_name)
            .unwrap_or(id.as_str());
        for tool in tools {
            let parameters = match sanitize_schema(&tool.input_schema) {
                Ok(parameters) => parameters,
                Err(issue) => {
                    super::super::overlay::push_log(format!(
                        "[mcp] quarantined unrepresentable tool schema: {id}/{} ({}, observed {}, limit {})",
                        tool.name, issue.reason, issue.observed, issue.limit
                    ));
                    super::super::telemetry::typed_error(
                        "ERR_MCP_TOOL_SCHEMA_UNREPRESENTABLE",
                        "mcp",
                        "an MCP tool was omitted because its input schema exceeds provider-wire bounds",
                        json!({
                            "integration_id": id,
                            "tool_name": tool.name,
                            "reason": issue.reason,
                            "observed": issue.observed,
                            "limit": issue.limit,
                        }),
                    );
                    continue;
                }
            };
            let name = unique_decl_name(id, &tool.name, &mut seen);
            routes.insert(
                name.clone(),
                ToolRoute {
                    integration_id: id.clone(),
                    tool_name: tool.name.clone(),
                    annotations: tool.annotations,
                    connection_token: *connection_token,
                },
            );
            declarations.push(json!({
                "name": name,
                "description": bounded_prose(&format!("{display}: {}", tool.description)),
                "parameters": parameters,
            }));
        }
    }
    (declarations, routes)
}

/// Return the connected tool's protocol-declared read-only effect. Missing,
/// malformed, stale, or destructive metadata remains conservative.
pub(in crate::overlay::computer_control) fn declared_tool_is_read_only(name: &str) -> Option<bool> {
    let manager = manager().lock();
    let route = manager.routes.get(name)?;
    let connection = manager.connected.get(&route.integration_id)?;
    route_read_only(route, connection_status(connection))
}

fn annotations_are_read_only(annotations: super::client::McpToolAnnotations) -> bool {
    annotations.read_only == Some(true) && annotations.destructive != Some(true)
}

fn route_is_current(route: &ToolRoute, connection: ConnectionStatus) -> bool {
    route.connection_token == connection.connection_token
        && connection.catalog_valid
        && connection.alive
}

fn route_read_only(route: &ToolRoute, connection: ConnectionStatus) -> Option<bool> {
    route_is_current(route, connection).then(|| annotations_are_read_only(route.annotations))
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
    let target = {
        let manager = manager().lock();
        let Some(route) = manager.routes.get(name) else {
            return Some(json!({"ok": false, "error": "mcp tool not currently available"}));
        };
        let connection = manager.connected.get(&route.integration_id);
        connection.and_then(|connection| {
            route_is_current(route, connection_status(connection))
                .then(|| (Arc::clone(&connection.client), route.tool_name.clone()))
        })
    };
    let Some((client, tool_name)) = target else {
        return Some(json!({"ok": false, "error": "mcp tool not currently available"}));
    };
    Some(match client.call_tool(&tool_name, args) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": format!("mcp call failed: {error:#}")}),
    })
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
