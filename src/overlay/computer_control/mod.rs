//! Computer Control — a Gemini Live screen+voice agent that drives Windows via
//! model tool calls. Development contract:
//! `docs/COMPUTER_CONTROL_DEVELOPMENT.md`.
//!
//! - `protocol` — setup payloads, tool declarations, the server-frame decoder.
//! - `session` — connect/capture/send primitives shared by runtime + probe.
//! - `executor` — `SendInput` mouse/keyboard with frame→screen coordinate mapping.
//! - `runtime` — the continuous session loop (mic + screen → tool calls → actions).
//! - `overlay` — the always-on-top status/action-log UI + session lifecycle.

// Templates have compile-time provenance; rendered arguments stay session-local.
macro_rules! cc_diagnostic {
    ($template:literal $($arg:tt)*) => {{
        $crate::overlay::computer_control::telemetry::diagnostic(
            $template, format_args!($template $($arg)*)
        );
    }};
}
macro_rules! cc_log {
    ($($arg:tt)*) => {{ cc_diagnostic!($($arg)*); }};
}

mod app_config;
mod artifacts;
mod browser;
#[cfg(test)]
mod capability_probe;
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
mod model_text_budget;
mod native_file;
mod orb;
mod overlay;
mod playback;
mod protocol;
mod research;
mod response_budget;
mod runtime;
mod session;
mod session_retention;
pub(crate) use crate::process_job as shell_job;
mod system_query;
mod telemetry;

pub(super) fn with_telemetry_paused<T>(operation: impl FnOnce() -> T) -> T {
    telemetry::with_closed_writers(operation)
}
mod turn_policy;
mod uia;
mod uia_task;
pub(crate) mod vision_contract;
mod vision_reader;

pub(crate) use external_control::ServerGuard as ExternalControlServerGuard;
/// MCP capability-store hooks for the Downloaded Tools settings UI (list/install/remove).
pub(crate) use mcp::{ui_install, ui_list, ui_remove, ui_remove_all};
pub use overlay::{is_active, show_overlay, stop_overlay};

/// "Clear all history" for Computer Control: saved conversation memory
/// (transcripts and screen frames) and per-session traces. The live session,
/// if any, stops persisting first so nothing it holds is written back; the
/// deletion itself runs off the calling (UI) thread.
pub(crate) fn clear_retained_history() {
    session_retention::forbid_active_session();
    std::thread::spawn(|| {
        if let Err(error) = memory::clear_all() {
            crate::log_info!("[cc] clearing conversation memory failed: {error}");
        }
        if let Err(error) = telemetry::clear_session_traces() {
            crate::log_info!("[cc] clearing session traces failed: {error}");
        }
    });
}

pub(crate) fn remove_downloaded_engine() -> anyhow::Result<()> {
    stop_overlay();
    engine::stop_for_component_removal();
    crate::component_registry::computer_control::remove()
}
