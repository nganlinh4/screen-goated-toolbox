mod activation;
mod button_input;
mod card_bridge;
mod card_document;
mod child;
mod controls;
mod delivery;
mod diagnostics;
pub(crate) mod font;
mod html;
mod isolated_server;
mod mailbox;
mod parent;
pub(crate) mod protocol;
mod region;
mod supervisor;
mod sync_scheduler;
mod web_response;
mod webview_failure;

pub use controls::{
    is_dragging, is_point_over_result_window, set_external_drag, set_opacity, set_refine_text,
    sync as sync_controls, sync_all as sync_all_controls,
};
pub use parent::{
    go_back, go_forward, raise_window, register_window, remove_window, sync_geometry, sync_window,
    update_theme, warmup,
};
pub(crate) use supervisor::wait_until_ready;
pub(crate) use sync_scheduler::queue_window_sync;

pub(crate) const CHILD_FLAG: &str = "--internal-result-compositor";

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|arg| arg == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}
