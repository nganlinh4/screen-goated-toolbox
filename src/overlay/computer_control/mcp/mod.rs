//! MCP capability store — curated, consent-gated app-control integrations that
//! extend the Computer Control agent's toolset. The model decides; code resolves +
//! installs + bridges. See the plan in `drifting-pondering-pixel.md`.
//!
//! Pipeline hooks: `active_tool_declarations()` is appended into `build_setup` (so a
//! connected integration's tools are declared to Gemini on (re)connect), and
//! `try_dispatch()` routes `mcp__id__tool` calls to the right server. Installing /
//! removing an integration sets `tools_changed()` so the runtime reconnects to pick
//! up the new tool set.

mod catalog;
mod client;
mod prefs;
mod registry;
mod support;

use client::{McpClient, McpTool};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    static M: std::sync::OnceLock<parking_lot::Mutex<Manager>> = std::sync::OnceLock::new();
    M.get_or_init(|| parking_lot::Mutex::new(Manager::default()))
}

/// Set when the connected set changes → the runtime reconnects so `build_setup`
/// re-declares the tools (Gemini Live freezes tools at session setup).
static TOOLS_CHANGED: AtomicBool = AtomicBool::new(false);
/// Emergency escape: if a reconnect's setup ever fails (a bad MCP schema), the runtime
/// flips this so the next `build_setup` omits MCP tools and the session always returns.
static SUPPRESS_TOOLS: AtomicBool = AtomicBool::new(false);

pub(super) fn tools_changed() -> bool {
    TOOLS_CHANGED.load(Ordering::SeqCst)
}
pub(super) fn clear_tools_changed() {
    TOOLS_CHANGED.store(false, Ordering::SeqCst);
}
pub(super) fn set_suppress_tools(on: bool) {
    SUPPRESS_TOOLS.store(on, Ordering::SeqCst);
}

pub(super) fn is_connected(id: &str) -> bool {
    manager()
        .lock()
        .connected
        .get(id)
        .is_some_and(|c| c.client.is_alive())
}

fn connected_snapshot(id: &str) -> Option<(Arc<McpClient>, Vec<McpTool>)> {
    manager()
        .lock()
        .connected
        .get(id)
        .filter(|c| c.client.is_alive())
        .map(|c| (c.client.clone(), c.tools.clone()))
}

/// Spawn + handshake + list tools, store the live client. Idempotent.
fn connect(id: &str) -> Result<usize, String> {
    if is_connected(id) {
        return Ok(0);
    }
    let integ = catalog::get(id).ok_or_else(|| format!("unknown integration '{id}'"))?;
    let client = McpClient::spawn(&integ.launch).map_err(|e| format!("spawn: {e:#}"))?;
    let tools = client
        .list_tools()
        .map_err(|e| format!("tools/list: {e:#}"))?;
    let count = tools.len();
    manager().lock().connected.insert(
        id.to_string(),
        Connected {
            client: Arc::new(client),
            tools,
        },
    );
    TOOLS_CHANGED.store(true, Ordering::SeqCst);
    Ok(count)
}

fn disconnect(id: &str) {
    if let Some(conn) = manager().lock().connected.remove(id) {
        conn.client.shutdown();
    }
    TOOLS_CHANGED.store(true, Ordering::SeqCst);
}

/// Best-effort: bring every installed integration back on session start (each on its
/// own thread, since a cold spawn can block). The runtime's reconnect-on-tools-changed
/// then declares their tools.
pub(super) fn connect_all_installed() {
    for id in registry::installed_ids() {
        std::thread::spawn(move || match connect(&id) {
            Ok(n) => eprintln!("[mcp] connected '{id}' ({n} tools)"),
            Err(e) => eprintln!("[mcp] connect '{id}' failed: {e}"),
        });
    }
}

/// Kill every server (session stop / app exit) so no child outlives the session.
pub(super) fn disconnect_all() {
    let mut m = manager().lock();
    for (_, conn) in m.connected.drain() {
        conn.client.shutdown();
    }
    m.routes.clear();
}

/// Gemini `functionDeclaration`s for every connected integration's tools, namespaced
/// `mcp__id__tool`. Rebuilds the dispatch route map as a side effect.
pub(super) fn active_tool_declarations() -> Vec<Value> {
    if SUPPRESS_TOOLS.load(Ordering::SeqCst) {
        return Vec::new();
    }
    let mut m = manager().lock();
    m.routes.clear();
    // Snapshot to avoid borrowing `connected` and `routes` at once.
    let snapshot: ConnSnapshot = m
        .connected
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                c.tools
                    .iter()
                    .map(|t| {
                        (
                            t.name.clone(),
                            t.description.clone(),
                            t.input_schema.clone(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (id, tools) in &snapshot {
        let display = catalog::get(id)
            .map(|i| i.display_name)
            .unwrap_or(id.as_str());
        for (tool_name, desc, schema) in tools {
            let name = unique_decl_name(id, tool_name, &mut seen);
            m.routes
                .insert(name.clone(), (id.clone(), tool_name.clone()));
            out.push(json!({
                "name": name,
                "description": format!("[{display}] {desc}"),
                "parameters": sanitize_schema(schema),
            }));
        }
    }
    out
}

/// Route an `mcp__…` tool call to its server. `None` = not an MCP call (let native
/// dispatch handle it); `Some(err json)` keeps a stale/dead call out of the native
/// "unknown action" path.
pub(super) fn try_dispatch(name: &str, args: &Value) -> Option<Value> {
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
        .map(|c| c.client.clone());
    let Some(client) = client else {
        return Some(json!({"ok": false, "error": format!("integration '{id}' not connected")}));
    };
    Some(match client.call_tool(&tool, args) {
        Ok(v) => v,
        Err(e) => json!({"ok": false, "error": format!("mcp call failed: {e:#}")}),
    })
}

// ── agent-facing management tools (dispatched from uia_task/dispatch.rs) ──────────

pub(super) fn list_tool() -> Value {
    let items: Vec<Value> = catalog::all()
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "name": i.display_name,
                "description": i.description,
                "publisher": i.publisher,
                "source": i.source_url,
                "needs_addon": i.addon_hint.is_some(),
                "installed": registry::is_installed(i.id),
                "connected": is_connected(i.id),
                "done_when": support::done_when(i.id),
            })
        })
        .collect();
    json!({"ok": true, "integrations": items})
}

/// Install + connect an integration. Refuses unless `confirmed` — it runs third-party
/// software, so the user must say yes first (the consequential-action gate).
pub(super) fn setup_tool(id: &str, confirmed: bool) -> Value {
    let Some(integ) = catalog::get(id) else {
        return json!({"ok": false, "error": format!("unknown integration '{id}'")});
    };
    if !confirmed {
        return json!({
            "ok": false, "need_confirm": true,
            "error": "this installs and runs third-party software - confirm with the user first, then call again with confirmed:true",
        });
    }
    if is_connected(id) {
        return json!({"ok": true, "note": "already set up and its tools are active"});
    }
    // Install + connect runs in the BACKGROUND and we return immediately, so a slow uvx
    // fetch doesn't block and the user's follow-up questions don't cancel it. The tools
    // activate via a reconnect the moment the conversation goes idle.
    let started = spawn_install(id);
    let note = if started {
        "Installing the MCP server in the background. Don't call this again - proceed."
    } else {
        "Already installing in the background - don't call this again, proceed."
    };
    let setup_id = format!("mcp-setup-{id}-{}", now_secs());
    let mut result = json!({
        "ok": true,
        "installing": true,
        "setup_id": setup_id,
        "id": id,
        "source_url": integ.source_url,
        "status_tool": "app_integration_status",
        "docs_tool": "read_app_integration_docs",
        "done_when": support::done_when(id),
        "note": note,
        "instruction": "This starts a bounded setup task. Check app_integration_status before claiming success, and stop if it reports ready. If app setup is needed, read_app_integration_docs and execute via programmatic surfaces before GUI clicks."
    });
    // If the integration needs in-app setup, the AGENT figures it out and does it ITSELF -
    // research the steps, then execute. No hand-written per-app recipe, no manual checklist
    // for the user.
    if let Some(need) = integ.addon_hint {
        result["agent_task"] = json!(format!(
            "This integration needs in-app setup and YOU do it yourself - never hand a manual checklist to the user. \
What's needed: {need} You are NOT given the steps - FIGURE THEM OUT: call read_app_integration_docs(id:'{id}') \
for the curated source docs, then carry it out with your own tools. PREFER the app's programmatic surface - its \
built-in scripting/Python console, a CLI, or editing add-on/config files directly - over clicking through GUI menus, \
which is slow and error-prone. Verify with app_integration_status(id:'{id}'); if it reports ready, stop setup. \
Only ask the user for something ONLY they can do, like a login or an OS permission dialog.",
            need = need
        ));
    }
    result
}

pub(super) fn status_tool(id: &str) -> Value {
    support::status_tool(id, connected_snapshot(id), tools_changed())
}

pub(super) fn docs_tool(id: &str) -> Value {
    support::docs_tool(id)
}

pub(super) fn remove_tool(id: &str) -> Value {
    disconnect(id);
    registry::remove(id);
    json!({"ok": true, "removed": id})
}

pub(super) fn decline_tool(id: &str) -> Value {
    prefs::record_decline(id);
    json!({"ok": true, "noted": "won't proactively offer that again for a while"})
}

/// The id of a curated integration whose app is in the foreground, NOT installed, and
/// NOT snoozed — drives the runtime's proactive offer. `None` = nothing to offer.
pub(super) fn detect_uninstalled_match(foreground_title: &str) -> Option<&'static str> {
    let title = foreground_title.to_lowercase();
    catalog::all().iter().find_map(|i| {
        let hit = !i.match_signals.is_empty()
            && !registry::is_installed(i.id)
            && prefs::offer_due(i.id)
            && i.match_signals
                .iter()
                .any(|s| title.contains(&s.to_lowercase()));
        hit.then_some(i.id)
    })
}

pub(super) fn display_name(id: &str) -> Option<&'static str> {
    catalog::get(id).map(|i| i.display_name)
}

// ── Downloaded-Tools UI surface (re-exported pub(crate) from computer_control) ────

/// Ids whose install thread is in flight (so the UI shows "installing…" and won't
/// double-spawn).
fn installing() -> &'static parking_lot::Mutex<HashSet<String>> {
    static S: std::sync::OnceLock<parking_lot::Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

/// One integration row for the settings panel.
pub(crate) struct UiIntegration {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub addon_hint: Option<&'static str>,
    pub installed: bool,
    pub connected: bool,
    pub installing: bool,
}

pub(crate) fn ui_list() -> Vec<UiIntegration> {
    let busy = installing().lock();
    catalog::all()
        .iter()
        .map(|i| UiIntegration {
            id: i.id,
            display_name: i.display_name,
            description: i.description,
            addon_hint: i.addon_hint,
            installed: registry::is_installed(i.id),
            connected: is_connected(i.id),
            installing: busy.contains(i.id),
        })
        .collect()
}

/// Kick off install + connect on a BACKGROUND thread (so a slow uvx fetch + handshake
/// can't block the agent or be cancelled by the user's next words). `false` = an install
/// for this id is already in flight.
fn spawn_install(id: &str) -> bool {
    if !installing().lock().insert(id.to_string()) {
        return false;
    }
    let id = id.to_string();
    std::thread::spawn(move || {
        match connect(&id) {
            Ok(n) => {
                registry::mark_installed(&id);
                eprintln!("[mcp] installed + connected '{id}' ({n} tools)");
            }
            Err(e) => eprintln!("[mcp] install '{id}' failed: {e}"),
        }
        installing().lock().remove(&id);
    });
    true
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Install + connect on a background thread (the UI button calls this). Idempotent.
pub(crate) fn ui_install(id: &str) {
    spawn_install(id);
}

pub(crate) fn ui_remove(id: &str) {
    disconnect(id);
    registry::remove(id);
}

/// Uninstall + forget everything (the panel's "Clean all").
pub(crate) fn ui_remove_all() {
    for id in registry::installed_ids() {
        disconnect(&id);
        registry::remove(&id);
    }
    prefs::clear();
}

// ── helpers ──────────────────────────────────────────────────────────────────────

/// Build the namespaced declared name, ≤64 chars (Gemini's limit), de-duped.
fn unique_decl_name(id: &str, tool: &str, seen: &mut HashSet<String>) -> String {
    let mut base: String = format!("mcp__{id}__{tool}").chars().take(64).collect();
    if !seen.contains(&base) {
        seen.insert(base.clone());
        return base;
    }
    let mut n = 2;
    loop {
        let suffix = format!("_{n}");
        let keep = 64usize.saturating_sub(suffix.len());
        base = format!(
            "{}{suffix}",
            format!("mcp__{id}__{tool}")
                .chars()
                .take(keep)
                .collect::<String>()
        );
        if seen.insert(base.clone()) {
            return base;
        }
        n += 1;
    }
}

/// Reduce an MCP JSON-Schema to the OpenAPI subset Gemini accepts (drops `$schema`,
/// `$defs`, `additionalProperties`, `format`, `title`, …); recurses into props/items.
fn sanitize_schema(schema: &Value) -> Value {
    let Value::Object(map) = schema else {
        return json!({"type": "object", "properties": {}});
    };
    let mut out = serde_json::Map::new();
    for (k, v) in map {
        match k.as_str() {
            "type" | "description" | "enum" | "required" => {
                out.insert(k.clone(), v.clone());
            }
            "properties" => {
                if let Value::Object(props) = v {
                    let p: serde_json::Map<String, Value> = props
                        .iter()
                        .map(|(pk, pv)| (pk.clone(), sanitize_schema(pv)))
                        .collect();
                    out.insert(k.clone(), Value::Object(p));
                }
            }
            "items" => {
                out.insert(k.clone(), sanitize_schema(v));
            }
            _ => {}
        }
    }
    if out.get("type").and_then(Value::as_str) == Some("object") && !out.contains_key("properties")
    {
        out.insert("properties".to_string(), json!({}));
    }
    if !out.contains_key("type") {
        out.insert("type".to_string(), json!("object"));
        out.entry("properties").or_insert_with(|| json!({}));
    }
    Value::Object(out)
}

/// Headless smoke test (`--cc-mcp-test <id>`): spawn the catalog's server for `id`,
/// list its tools, then either call an explicit tool or run the generic semantic
/// health probe. Verifies the stdio JSON-RPC bridge end to end with NO Gemini.
pub fn run_mcp_test(
    id: &str,
    tool: Option<&str>,
    args_json: Option<&str>,
    list_only: bool,
) -> Result<(), String> {
    let integ = catalog::get(id).ok_or_else(|| format!("unknown integration '{id}'"))?;
    eprintln!(
        "[mcp-test] spawning {} via '{}'...",
        integ.display_name, integ.launch.program
    );
    let client = McpClient::spawn(&integ.launch).map_err(|e| format!("spawn: {e:#}"))?;

    let tools = client
        .list_tools()
        .map_err(|e| format!("tools/list: {e:#}"))?;
    eprintln!("[mcp-test] {} tools:", tools.len());
    for t in &tools {
        let schema: String = serde_json::to_string(&t.input_schema)
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect();
        eprintln!(
            "  - {} : {} | schema {schema}",
            t.name,
            t.description.chars().take(80).collect::<String>()
        );
    }
    if list_only {
        eprintln!("[mcp-test] list-only; shutting the server down");
        client.shutdown();
        return Ok(());
    }
    if let Some(tool_name) = tool {
        let args = match args_json {
            Some(s) => serde_json::from_str::<Value>(s).map_err(|e| format!("bad args JSON: {e}"))?,
            None => json!({}),
        };
        eprintln!("[mcp-test] calling {tool_name} {args}...");
        let result = client
            .call_tool(tool_name, &args)
            .map_err(|e| format!("tool call failed: {e:#}"))?;
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
        let health = support::semantic_health(&client, &tools);
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
