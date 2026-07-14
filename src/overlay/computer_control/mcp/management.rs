use super::{catalog, install, registry, runtime, support};
use serde_json::{Value, json};

pub(in crate::overlay::computer_control) fn list_tool() -> Value {
    let items: Vec<Value> = catalog::all()
        .iter()
        .map(|integration| {
            json!({
                "id": integration.id,
                "name": integration.display_name,
                "description": integration.description,
                "publisher": integration.publisher,
                "source": integration.source_url,
                "needs_addon": integration.addon_hint.is_some(),
                "installed": registry::is_installed(integration.id),
                "connected": super::is_connected(integration.id),
                "done_when": support::done_when(integration.id),
            })
        })
        .collect();
    json!({"ok": true, "integrations": items})
}

/// Install + connect an integration. Refuses unless `confirmed` — it runs third-party
/// software, so the user must say yes first (the consequential-action gate).
pub(in crate::overlay::computer_control) fn setup_tool(id: &str, confirmed: bool) -> Value {
    let Some(integration) = catalog::get(id) else {
        return json!({"ok": false, "error": format!("unknown integration '{id}'")});
    };
    if !confirmed {
        return json!({
            "ok": false, "need_confirm": true,
            "error": "this installs and runs third-party software - confirm with the user first, then call again with confirmed:true",
        });
    }
    if super::is_connected(id) {
        return json!({"ok": true, "note": "already set up and its tools are active"});
    }
    // Install + connect runs in the BACKGROUND and we return immediately, so a slow uvx
    // fetch doesn't block and the user's follow-up questions don't cancel it. The tools
    // activate via a reconnect the moment the conversation goes idle.
    let started = install::spawn(id);
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
        "source_url": integration.source_url,
        "status_tool": "app_integration_status",
        "docs_tool": "read_app_integration_docs",
        "done_when": support::done_when(id),
        "note": note,
        "instruction": "This starts a bounded setup task. Check app_integration_status before claiming success, and stop if it reports ready. If app setup is needed, read_app_integration_docs and execute via programmatic surfaces before GUI clicks."
    });
    // If the integration needs in-app setup, the AGENT figures it out and does it ITSELF -
    // research the steps, then execute. No hand-written per-app recipe, no manual checklist
    // for the user.
    if let Some(need) = integration.addon_hint {
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

pub(in crate::overlay::computer_control) fn status_tool(id: &str) -> Value {
    support::status_tool(
        id,
        runtime::connected_snapshot(id),
        runtime::tools_changed(),
    )
}

pub(in crate::overlay::computer_control) fn docs_tool(id: &str) -> Value {
    support::docs_tool(id)
}

pub(in crate::overlay::computer_control) fn remove_tool(id: &str) -> Value {
    runtime::disconnect(id);
    registry::remove(id);
    json!({"ok": true, "removed": id})
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
