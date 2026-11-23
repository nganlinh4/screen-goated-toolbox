pub mod app_selection;
mod child;
pub mod controller;
mod document;
pub(crate) mod download;
mod event_worker;
mod ipc;
mod layout;
mod mailbox;
pub mod manager;
mod parent;
pub mod protocol;
pub(crate) mod smoke;
pub mod state;
mod supervisor;
pub(crate) mod target_change;
mod text_edit;
mod text_input_focus;
pub mod webview;
mod webview_failure;
pub mod wndproc;

pub use manager::{is_realtime_overlay_active, show_realtime_overlay, stop_realtime_overlay};
pub use state::*;

pub(crate) const CHILD_FLAG: &str = "--internal-realtime-compositor";

pub(crate) fn is_child_process() -> bool {
    std::env::args_os().any(|argument| argument == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    manager::run_child()
}

pub(crate) fn shutdown_for_exit() {
    supervisor::shutdown_for_exit();
}
