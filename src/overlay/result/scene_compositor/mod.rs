mod card_bridge;
mod card_document;
mod child;
mod delivery;
mod diagnostics;
pub(crate) mod font;
mod html;
mod isolated_server;
mod parent;
mod protocol;
mod sync_scheduler;

pub(crate) use parent::wait_until_ready;
pub use parent::{
    go_back, go_forward, raise_window, register_window, remove_window, sync_geometry, sync_window,
    update_theme, warmup,
};
pub(crate) use sync_scheduler::queue_window_sync;

pub(crate) const CHILD_FLAG: &str = "--internal-result-compositor";

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|arg| arg == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}
