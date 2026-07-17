//! MCP capability store for curated external integrations. Connected servers expose
//! their complete live tool catalog; the model selects semantics while code handles
//! explicit setup/removal requests, lifecycle, schema transport, and exact routing.
//!
//! Pipeline hooks: `active_tool_declarations()` is appended into `build_setup` (so a
//! connected integration's tools are declared to Gemini on (re)connect), and
//! `try_dispatch()` routes `mcp__id__tool` calls to the right server. Installing /
//! removing an integration sets `tools_changed()` so the runtime reconnects to pick
//! up the new tool set.

mod catalog;
mod client;
mod client_protocol;
mod install;
mod management;
mod registry;
mod runtime;
mod schema;
mod smoke;
mod startup;
mod support;
mod ui;

pub(super) use management::{docs_tool, list_tool, remove_tool, setup_tool, status_tool};
pub(super) use runtime::{
    active_tool_declarations, clear_tools_changed, connect_all_installed,
    declared_tool_is_read_only, disconnect_all, is_connected, tools_changed, try_dispatch,
};
pub(super) use smoke::run_mcp_test;
pub(super) use startup::StartupCatalog;
pub(crate) use ui::{UiIntegration, ui_install, ui_remove, ui_remove_all};

pub(crate) fn ui_list() -> Vec<UiIntegration> {
    ui::ui_list()
}
