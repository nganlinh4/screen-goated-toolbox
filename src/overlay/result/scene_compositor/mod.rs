mod card_bridge;
mod child;
mod diagnostics;
mod html;
mod parent;
mod protocol;

pub use parent::{go_back, go_forward, register_window, remove_window, sync_geometry, sync_window};

pub(crate) const CHILD_FLAG: &str = "--internal-result-compositor";

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|arg| arg == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}
