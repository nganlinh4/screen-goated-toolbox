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
mod install;
mod management;
mod registry;
mod runtime;
mod schema;
mod smoke;
mod support;
mod ui;

pub(super) use management::{docs_tool, list_tool, remove_tool, setup_tool, status_tool};
pub(super) use runtime::{
    StartupCatalog, active_tool_declarations, call_tool, clear_tools_changed,
    connect_all_installed, disconnect_all, is_connected, search_tools, tools_changed, try_dispatch,
};
pub(super) use smoke::run_mcp_test;
pub(crate) use ui::{UiIntegration, ui_install, ui_remove, ui_remove_all};

pub(crate) fn ui_list() -> Vec<UiIntegration> {
    ui::ui_list()
}
