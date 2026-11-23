use windows::Win32::Foundation::HWND;
use wry::{WebView, WebViewExtWindows};

use super::protocol::ChildEvent;

pub(super) fn attach(hwnd: HWND, webview: &WebView) {
    crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
        "realtime-compositor",
        hwnd,
        webview,
    );
    let core = webview.webview();
    if let Err(error) = crate::overlay::compositor_renderer::attach_process_failure(&core, |kind| {
        super::child::emit_event(ChildEvent::RendererFailure { kind })
    }) {
        eprintln!("process-failure handler attach failed: {error:?}");
    }
}
