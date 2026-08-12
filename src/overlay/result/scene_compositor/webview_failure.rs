use super::protocol::{ChildEvent, RendererFailureKind};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    },
    ProcessFailedEventHandler,
};
use windows::Win32::Foundation::HWND;
use wry::{WebView, WebViewExtWindows};

pub(super) fn attach(hwnd: HWND, webview: &WebView) {
    crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
        "result-compositor",
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
            crate::log_info!(
                "[ResultCompositor] process-failure handler attach failed error={error:?}"
            );
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
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED,
    };

    #[test]
    fn only_failures_that_need_recovery_or_gpu_escalation_cross_the_pipe() {
        assert_eq!(
            actionable_kind(COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED),
            Some(RendererFailureKind::GpuProcessExited)
        );
        assert_eq!(
            actionable_kind(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED),
            Some(RendererFailureKind::RenderProcessExited)
        );
        assert_eq!(
            actionable_kind(COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED),
            None
        );
        assert_eq!(actionable_kind(COREWEBVIEW2_PROCESS_FAILED_KIND(99)), None);
    }
}
