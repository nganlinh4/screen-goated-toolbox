pub(super) mod acceptance_capture;
pub(super) mod activation;
mod button_input;
mod card_bridge;
mod card_document;
mod child;
mod control_surface;
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
    is_dragging, is_point_over_result_window, set_control_scope_opacity, set_external_drag,
    set_refine_text, sync as sync_controls, sync_all as sync_all_controls,
    update_cached_refine_draft,
};
pub use parent::{
    go_back, go_forward, raise_window, register_window, remove_window, sync_geometry, sync_window,
    update_theme, warmup,
};
pub(crate) use supervisor::{restart_and_wait, wait_until_ready};
pub(crate) use sync_scheduler::queue_window_sync;

pub(crate) fn shutdown_for_exit() {
    supervisor::shutdown_for_exit();
}

pub(crate) const CHILD_FLAG: &str = "--internal-result-compositor";
const OFFSCREEN_ACCEPTANCE_ENV: &str = "SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN";

pub(crate) fn acceptance_offscreen() -> bool {
    std::env::var(OFFSCREEN_ACCEPTANCE_ENV).as_deref() == Ok("1")
}

pub(crate) fn compositor_host_x(virtual_x: i32, width: i32) -> i32 {
    host_x_for_mode(virtual_x, width, acceptance_offscreen())
}

fn host_x_for_mode(virtual_x: i32, width: i32, offscreen: bool) -> i32 {
    if offscreen {
        virtual_x.saturating_sub(width).saturating_sub(4096)
    } else {
        virtual_x
    }
}

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|arg| arg == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}

#[cfg(test)]
mod tests {
    use super::host_x_for_mode;

    #[test]
    fn acceptance_renderer_is_placed_beyond_the_virtual_desktop() {
        assert_eq!(host_x_for_mode(0, 2560, false), 0);
        assert_eq!(host_x_for_mode(-1920, 4480, true), -10_496);
    }
}
