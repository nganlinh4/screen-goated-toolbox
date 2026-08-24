use webview2_com::{
    CoTaskMemPWSTR,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        COREWEBVIEW2_PROCESS_FAILED_REASON, ICoreWebView2ProcessFailedEventArgs,
        ICoreWebView2ProcessFailedEventArgs2, ICoreWebView2ProcessFailedEventArgs3,
    },
    ProcessFailedEventHandler,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
use windows::core::{Interface, PWSTR};
use wry::{WebView, WebViewExtWindows};

pub fn attach_webview2_diagnostics(label: &'static str, hwnd: HWND, webview: &WebView) {
    attach(label, hwnd, webview, false);
}

pub fn attach_webview2_close_on_failure(label: &'static str, hwnd: HWND, webview: &WebView) {
    attach(label, hwnd, webview, true);
}

fn attach(label: &'static str, hwnd: HWND, webview: &WebView, close_on_failure: bool) {
    let core = webview.webview();
    let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
        let actionable = args
            .as_ref()
            .map(process_failed_kind)
            .is_some_and(is_actionable_host_failure);
        log_process_failed(label, hwnd, args);
        if close_on_failure && actionable {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        Ok(())
    }));

    let mut token = 0i64;
    unsafe {
        match core.add_ProcessFailed(&handler, &mut token) {
            Ok(()) => {}
            Err(err) => {
                crate::log_info!(
                    "[WebView2Diag] process-failed-handler-attach-failed label={} hwnd={:?} error={:?}",
                    label,
                    hwnd,
                    err
                );
            }
        }
    }
}

fn is_actionable_host_failure(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> bool {
    matches!(
        kind,
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
            | COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
            | COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
            | COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED
    )
}

unsafe fn read_pwstr<F>(read: F) -> Option<String>
where
    F: FnOnce(*mut PWSTR) -> windows::core::Result<()>,
{
    let mut value = PWSTR(std::ptr::null_mut());
    if read(&mut value).is_err() || value.is_null() {
        return None;
    }

    Some(CoTaskMemPWSTR::from(value).to_string())
}

fn log_process_failed(
    label: &'static str,
    hwnd: HWND,
    args: Option<ICoreWebView2ProcessFailedEventArgs>,
) {
    let Some(args) = args else {
        crate::log_info!(
            "[WebView2Diag] process-failed label={} hwnd={:?} args=none",
            label,
            hwnd
        );
        return;
    };

    let kind = process_failed_kind(&args);
    let args2 = args.cast::<ICoreWebView2ProcessFailedEventArgs2>().ok();
    let args3 = args.cast::<ICoreWebView2ProcessFailedEventArgs3>().ok();

    let reason = args2.as_ref().and_then(process_failed_reason);
    let exit_code = args2.as_ref().and_then(process_failed_exit_code);
    let description = args2.as_ref().and_then(process_description);
    let module_path = args3.as_ref().and_then(failure_source_module_path);

    crate::log_info!(
        "[WebView2Diag] process-failed label={} hwnd={:?} kind={:?} kind_name={} reason={:?} reason_name={} exit_code={:?} process={} failure_module={}",
        label,
        hwnd,
        kind,
        kind_name(kind),
        reason,
        reason.map(reason_name).unwrap_or("unknown"),
        exit_code,
        description.unwrap_or_else(|| "unavailable".to_string()),
        module_path.unwrap_or_else(|| "unavailable".to_string()),
    );
}

fn process_failed_kind(
    args: &ICoreWebView2ProcessFailedEventArgs,
) -> COREWEBVIEW2_PROCESS_FAILED_KIND {
    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
    unsafe {
        let _ = args.ProcessFailedKind(&mut kind);
    }
    kind
}

fn process_failed_reason(
    args: &ICoreWebView2ProcessFailedEventArgs2,
) -> Option<COREWEBVIEW2_PROCESS_FAILED_REASON> {
    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
    unsafe {
        args.Reason(&mut reason).ok()?;
    }
    Some(reason)
}

fn process_failed_exit_code(args: &ICoreWebView2ProcessFailedEventArgs2) -> Option<i32> {
    let mut exit_code = 0;
    unsafe {
        args.ExitCode(&mut exit_code).ok()?;
    }
    Some(exit_code)
}

fn process_description(args: &ICoreWebView2ProcessFailedEventArgs2) -> Option<String> {
    unsafe { read_pwstr(|value| args.ProcessDescription(value)) }
}

fn failure_source_module_path(args: &ICoreWebView2ProcessFailedEventArgs3) -> Option<String> {
    unsafe { read_pwstr(|value| args.FailureSourceModulePath(value)) }
}

fn kind_name(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> &'static str {
    match kind.0 {
        0 => "browser_process_exited",
        1 => "render_process_exited",
        2 => "render_process_unresponsive",
        3 => "frame_render_process_exited",
        4 => "utility_process_exited",
        5 => "sandbox_helper_process_exited",
        6 => "gpu_process_exited",
        7 => "ppapi_plugin_process_exited",
        8 => "ppapi_broker_process_exited",
        9 => "unknown_process_exited",
        _ => "unknown",
    }
}

fn reason_name(reason: COREWEBVIEW2_PROCESS_FAILED_REASON) -> &'static str {
    match reason.0 {
        0 => "unexpected",
        1 => "unresponsive",
        2 => "terminated",
        3 => "crashed",
        4 => "launch_failed",
        5 => "out_of_memory",
        6 => "profile_deleted",
        _ => "unknown",
    }
}
