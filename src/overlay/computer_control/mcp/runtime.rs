use super::catalog;
use super::client::{McpClient, McpTool};
use super::client_protocol::{ClientLifecycleEvents, ClientLifecycleKind};
use super::registry;
use super::startup::{StartupAttempt, StartupCatalog};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

mod capacity;
#[cfg(test)]
mod declaration_test_support;
mod declarations;
#[cfg(test)]
use declaration_test_support::direct_declarations;
mod normalization;

/// A connected integration: its live client + the tool list it advertised.
struct Connected {
    client: Arc<McpClient>,
    tools: Arc<Vec<McpTool>>,
    catalog_valid: bool,
}

#[derive(Default)]
struct Manager {
    revision: u64,
    catalog_builder: Arc<parking_lot::Mutex<()>>,
    connected: HashMap<String, Connected>,
    /// Declared name → exact dispatch route plus protocol-authored effect metadata.
    routes: HashMap<String, ToolRoute>,
    catalog_issue: Option<capacity::CatalogIssue>,
}

impl Manager {
    fn catalog_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
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
    is_connected_in(manager(), id)
}

pub(super) fn catalog_error(id: &str) -> Option<String> {
    catalog_error_in(manager(), id)
}

fn catalog_error_in(store: &parking_lot::Mutex<Manager>, id: &str) -> Option<String> {
    let manager = store.lock();
    manager
        .catalog_issue
        .as_ref()
        .filter(|issue| {
            issue
                .integration_ids
                .iter()
                .any(|integration| integration == id)
        })
        .map(|issue| issue.detail.clone())
}

fn is_connected_in(store: &parking_lot::Mutex<Manager>, id: &str) -> bool {
    store
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
    let (client, tools) = {
        let manager = manager().lock();
        let connection = manager.connected.get(id)?;
        if !connection.catalog_valid || !connection.client.is_alive() {
            return None;
        }
        (connection.client.clone(), connection.tools.clone())
    };
    Some((client, tools.as_ref().clone()))
}

pub(super) fn connect_owned(id: &str, stop: &AtomicBool) -> Result<usize, String> {
    connect_inner(id, Some(stop))?.ok_or_else(|| "connection owner stopped".to_string())
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
    let registered = register_connection(manager(), id, &client, tools, stop);
    if !registered {
        return Ok(None);
    }
    spawn_lifecycle_worker(id.to_string(), connection_token, lifecycle);
    mark_catalog_changed();
    Ok(Some(count))
}

fn register_connection(
    store: &parking_lot::Mutex<Manager>,
    id: &str,
    client: &Arc<McpClient>,
    tools: Vec<McpTool>,
    stop: Option<&AtomicBool>,
) -> bool {
    let tools = Arc::new(tools);
    let mut displaced = None;
    let registered = {
        let mut manager = store.lock();
        register_if_owner_active(stop, || {
            displaced = manager.connected.insert(
                id.to_string(),
                Connected {
                    client: client.clone(),
                    tools: tools.clone(),
                    catalog_valid: true,
                },
            );
            manager.routes.retain(|_, route| route.integration_id != id);
            manager.catalog_changed();
        })
    };
    drop(displaced);
    registered
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
    std::thread::spawn(super::super::session_retention::inherit(move || {
        consume_lifecycle_events(lifecycle, connection_token, |kind| match kind {
            ClientLifecycleKind::ToolsChanged => refresh_catalog(&integration_id, connection_token),
            ClientLifecycleKind::Disconnected => {
                remove_current_connection(&integration_id, connection_token);
            }
        });
    }));
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
    let Some(client) = invalidate_catalog(manager(), integration_id, connection_token) else {
        return;
    };
    mark_catalog_changed();

    match client.list_tools() {
        Ok(tools) => replace_current_catalog(integration_id, connection_token, tools),
        Err(error) => {
            cc_diagnostic!("[mcp] refresh '{integration_id}' failed: {error:#}");
            remove_current_connection(integration_id, connection_token);
            client.shutdown();
        }
    }
}

fn invalidate_catalog(
    store: &parking_lot::Mutex<Manager>,
    id: &str,
    token: u64,
) -> Option<Arc<McpClient>> {
    let mut manager = store.lock();
    let connection = manager.connected.get_mut(id)?;
    if connection.client.connection_token() != token {
        return None;
    }
    connection.catalog_valid = false;
    let client = connection.client.clone();
    manager
        .routes
        .retain(|_, route| route.connection_token != token);
    manager.catalog_changed();
    Some(client)
}

fn replace_current_catalog(integration_id: &str, connection_token: u64, tools: Vec<McpTool>) {
    if replace_catalog(manager(), integration_id, connection_token, tools) {
        mark_catalog_changed();
    }
}

fn replace_catalog(
    store: &parking_lot::Mutex<Manager>,
    id: &str,
    token: u64,
    tools: Vec<McpTool>,
) -> bool {
    let replacement = Arc::new(tools);
    let previous = {
        let mut manager = store.lock();
        let Some(connection) = manager.connected.get_mut(id) else {
            return false;
        };
        if refresh_disposition(Some(connection_status(connection)), token, true)
            != RefreshDisposition::Replace
        {
            return false;
        }
        let previous = std::mem::replace(&mut connection.tools, replacement);
        connection.catalog_valid = true;
        manager
            .routes
            .retain(|_, route| route.connection_token != token);
        manager.catalog_changed();
        previous
    };
    drop(previous);
    true
}

fn remove_current_connection(integration_id: &str, connection_token: u64) {
    let removed = detach_connection(manager(), integration_id, Some(connection_token));
    if removed.is_some() {
        mark_catalog_changed();
    }
    drop(removed);
}

fn detach_connection(
    store: &parking_lot::Mutex<Manager>,
    id: &str,
    token: Option<u64>,
) -> Option<Connected> {
    let mut manager = store.lock();
    if token.is_none()
        && manager.catalog_issue.as_ref().is_some_and(|issue| {
            issue
                .integration_ids
                .iter()
                .any(|integration| integration == id)
        })
    {
        manager.catalog_issue = None;
    }
    let current = manager.connected.get(id)?;
    if token.is_some_and(|token| current.client.connection_token() != token) {
        return None;
    }
    let removed = manager.connected.remove(id);
    if manager.catalog_issue.as_ref().is_some_and(|issue| {
        issue
            .integration_ids
            .iter()
            .any(|integration| integration == id)
    }) {
        manager.catalog_issue = None;
    }
    manager.routes.retain(|_, route| route.integration_id != id);
    manager.catalog_changed();
    removed
}

pub(super) fn disconnect(id: &str) {
    if let Some(connection) = detach_connection(manager(), id, None) {
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
        std::thread::spawn(super::super::session_retention::inherit(move || {
            let outcome = match connect_inner(&id, Some(&stop)) {
                Ok(Some(count)) => {
                    cc_diagnostic!("[mcp] connected '{id}' ({count} tools)");
                    StartupAttempt::Connected
                }
                Ok(None) => StartupAttempt::Stopped,
                Err(error) => {
                    if stop.load(Ordering::SeqCst) {
                        StartupAttempt::Stopped
                    } else {
                        cc_diagnostic!("[mcp] connect '{id}' failed: {error}");
                        StartupAttempt::Failed
                    }
                }
            };
            let _ = tx.send(outcome);
        }));
    }
    drop(tx);
    StartupCatalog::new(installed, attempts)
}

/// Kill every server (session stop / app exit) so no child outlives the session.
pub(in crate::overlay::computer_control) fn disconnect_all() {
    if disconnect_all_in(manager(), |connection| connection.client.shutdown()) {
        mark_catalog_changed();
    }
}

fn disconnect_all_in(
    store: &parking_lot::Mutex<Manager>,
    mut shutdown: impl FnMut(Connected),
) -> bool {
    let (connections, routes, changed) = {
        let mut manager = store.lock();
        let changed = !manager.connected.is_empty() || !manager.routes.is_empty();
        let connections = std::mem::take(&mut manager.connected);
        let routes = std::mem::take(&mut manager.routes);
        if changed {
            manager.catalog_changed();
        }
        (connections, routes, changed)
    };
    drop(routes);
    for connection in connections.into_values() {
        shutdown(connection);
    }
    changed
}

/// Declare every tool exposed by every live connection. The model receives the
/// complete capability catalog and owns semantic selection; Rust only namespaces
/// names, sanitizes wire schemas, and records exact dispatch routes.
pub(in crate::overlay::computer_control) fn active_tool_declarations() -> anyhow::Result<Vec<Value>>
{
    declarations::build(manager(), normalization::engine_declarations)
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
    cancel: &Arc<AtomicBool>,
) -> Option<Value> {
    dispatch_in(manager(), name, args, cancel)
}

fn dispatch_in(
    store: &parking_lot::Mutex<Manager>,
    name: &str,
    args: &Value,
    cancel: &Arc<AtomicBool>,
) -> Option<Value> {
    if !name.starts_with("mcp__") {
        return None;
    }
    let target = {
        let manager = store.lock();
        let Some(route) = manager.routes.get(name) else {
            return Some(unavailable_tool());
        };
        let connection = manager.connected.get(&route.integration_id);
        connection.and_then(|connection| {
            route_is_current(route, connection_status(connection))
                .then(|| (Arc::clone(&connection.client), route.tool_name.clone()))
        })
    };
    let Some((client, tool_name)) = target else {
        return Some(unavailable_tool());
    };
    Some(match client.call_tool_owned(&tool_name, args, cancel) {
        Ok(value) => value,
        Err(error) => super::client::format_error(&error),
    })
}

fn unavailable_tool() -> Value {
    let mut result = json!({"ok":false,"error":"mcp tool not currently available"});
    crate::overlay::computer_control::effect_receipt::EffectStatus::ProvenNoEffect
        .annotate(&mut result);
    result
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime/concurrency_tests.rs"]
mod concurrency_tests;

#[cfg(test)]
#[path = "runtime/cancellation_tests.rs"]
mod cancellation_tests;

#[cfg(test)]
#[path = "runtime/capacity_tests.rs"]
mod capacity_tests;
