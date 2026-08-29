//! Computer Control — a Gemini Live screen+voice agent that drives Windows via
//! model tool calls. Development contract:
//! `docs/COMPUTER_CONTROL_DEVELOPMENT.md`.
//!
//! - `protocol` — setup payloads, tool declarations, the server-frame decoder.
//! - `session` — connect/capture/send primitives shared by runtime + probe.
//! - `executor` — `SendInput` mouse/keyboard with frame→screen coordinate mapping.
//! - `runtime` — the continuous session loop (mic + screen → tool calls → actions).
//! - `overlay` — the always-on-top status/action-log UI + session lifecycle.

mod artifacts;
mod browser;
mod clipboard;
mod controller;
mod effect_receipt;
mod engine;
mod executor;
mod external_control;
mod grid;
mod human_input;
mod mcp;
mod memory;
mod orb;
mod overlay;
mod playback;
mod protocol;
mod research;
mod runtime;
mod session;
mod system_query;
mod telemetry;
mod turn_policy;
mod uia;
mod uia_task;
pub(crate) mod vision_contract;
mod vision_reader;

pub(crate) use external_control::ServerGuard as ExternalControlServerGuard;
/// MCP capability-store hooks for the Downloaded Tools settings UI (list/install/remove).
pub(crate) use mcp::{ui_install, ui_list, ui_remove, ui_remove_all};
pub use overlay::{is_active, show_overlay, stop_overlay};

pub(crate) fn remove_downloaded_engine() -> anyhow::Result<()> {
    stop_overlay();
    engine::stop_for_component_removal();
    crate::component_registry::computer_control::remove()
}
