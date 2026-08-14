use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
};
use webview2_com::ProcessFailedEventHandler;
use windows::Win32::Foundation::HWND;
use wry::{WebView, WebViewExtWindows};

use super::protocol::{ChildEvent, RendererFailureKind};

pub(super) fn attach(hwnd: HWND, webview: &WebView) {
    crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
        "realtime-compositor",
        hwnd,
        webview,
    );
    let core = webview.webview();
    let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut raw_kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
        unsafe {
            args.ProcessFailedKind(&mut raw_kind)?;
        }
        if let Some(kind) = actionable_kind(raw_kind) {
            super::child::emit_event(ChildEvent::RendererFailure { kind });
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe {
        if let Err(error) = core.add_ProcessFailed(&handler, &mut token) {
            eprintln!("process-failure handler attach failed: {error:?}");
        }
    }
}

fn actionable_kind(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> Option<RendererFailureKind> {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => {
            Some(RendererFailureKind::BrowserProcessExited)
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => {
            Some(RendererFailureKind::RenderProcessExited)
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => {
            Some(RendererFailureKind::RenderProcessUnresponsive)
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
            Some(RendererFailureKind::FrameRenderProcessExited)
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => {
            Some(RendererFailureKind::GpuProcessExited)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_actionable_failures_restart_the_renderer() {
        assert_eq!(
            actionable_kind(COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED),
            Some(RendererFailureKind::GpuProcessExited)
        );
    }
}
