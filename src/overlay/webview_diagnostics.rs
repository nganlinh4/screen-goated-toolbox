use webview2_com::{
    CoTaskMemPWSTR,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_REASON,
        ICoreWebView2Environment, ICoreWebView2Environment11, ICoreWebView2ProcessFailedEventArgs,
        ICoreWebView2ProcessFailedEventArgs2, ICoreWebView2ProcessFailedEventArgs3,
    },
    ProcessFailedEventHandler,
};
use windows::Win32::Foundation::HWND;
use windows061::core::{Interface, PWSTR};
use wry::{WebView, WebViewExtWindows};

pub fn attach_webview2_diagnostics(label: &'static str, hwnd: HWND, webview: &WebView) {
    let env = webview.environment();
    let version = browser_version(&env).unwrap_or_else(|| "unknown".to_string());
    let failure_report_dir =
        failure_report_folder(&env).unwrap_or_else(|| "unavailable".to_string());

    crate::log_info!(
        "[WebView2Diag] attach label={} hwnd={:?} version={} failure_reports={}",
        label,
        hwnd,
        version,
        failure_report_dir
    );

    let core = webview.webview();
    let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
        log_process_failed(label, hwnd, args);
        Ok(())
    }));

    let mut token = 0i64;
    unsafe {
        match core.add_ProcessFailed(&handler, &mut token) {
            Ok(()) => {
                crate::log_info!(
                    "[WebView2Diag] process-failed-handler-attached label={} hwnd={:?} token={}",
                    label,
                    hwnd,
                    token
                );
            }
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

fn browser_version(env: &ICoreWebView2Environment) -> Option<String> {
    unsafe { read_pwstr(|value| env.BrowserVersionString(value)) }
}

fn failure_report_folder(env: &ICoreWebView2Environment) -> Option<String> {
    let env11 = env.cast::<ICoreWebView2Environment11>().ok()?;
    unsafe { read_pwstr(|value| env11.FailureReportFolderPath(value)) }
}

unsafe fn read_pwstr<F>(read: F) -> Option<String>
where
    F: FnOnce(*mut PWSTR) -> windows061::core::Result<()>,
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
