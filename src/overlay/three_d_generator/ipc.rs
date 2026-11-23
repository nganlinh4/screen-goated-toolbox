pub(super) use crate::overlay::creation_commands::{Envelope as IpcEnvelope, reply_script};
use crate::overlay::creation_error::CreationError;
use crate::overlay::generation_history::commands::ResultPathHooks;
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_MINIMIZE, ShowWindow, WM_CLOSE};

pub(super) fn handle_ipc(hwnd: HWND, body: &str) {
    let envelope = match parse_request(body) {
        Ok(envelope) => envelope,
        Err(diagnostic) => {
            crate::log_info!("{diagnostic}");
            return;
        }
    };
    if envelope.cmd == "read_image_preview" {
        if let Err(error) = request_image_preview(hwnd, &envelope) {
            send_reply(&envelope.cmd, &envelope.id, Err(error.into()));
        }
        return;
    }
    if let Some(reply) = dispatch_window(hwnd, &envelope.cmd) {
        send_reply(&envelope.cmd, &envelope.id, reply.map_err(Into::into));
        return;
    }
    let (id, cmd) = (envelope.id.clone(), envelope.cmd.clone());
    if let Err(error) = super::ipc_worker::request(hwnd, envelope, body.len()) {
        send_reply(&cmd, &id, Err(error));
    }
}

/// Admits one WebView message, or returns the diagnostic explaining why it was
/// dropped. The body itself is never echoed into the log.
fn parse_request(body: &str) -> Result<IpcEnvelope, String> {
    if body.len() > super::ipc_worker::MAX_REQUEST_BYTES {
        return Err(format!(
            "[3D Generator] Ignored a {}-byte request above the {}-byte limit",
            body.len(),
            super::ipc_worker::MAX_REQUEST_BYTES
        ));
    }
    serde_json::from_str(body).map_err(|error| format!("[3D Generator] Invalid request: {error}"))
}

/// The UI receives only a stable failure code, so the cause is logged here.
fn failure_diagnostic(cmd: &str, result: &Result<Value, CreationError>) -> Option<String> {
    result
        .as_ref()
        .err()
        .map(|error| format!("[3D Generator] {cmd} failed: {error}"))
}

fn request_image_preview(hwnd: HWND, envelope: &IpcEnvelope) -> Result<(), String> {
    let path = envelope
        .args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path is required".to_string())?;
    let max_edge = envelope
        .args
        .get("maxEdge")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    crate::overlay::creation_preview::request_async_preview(
        hwnd,
        super::WM_APP_PREVIEW_REPLY,
        envelope.id.clone(),
        path.to_string(),
        max_edge,
    )
}

pub(super) fn flush_preview_replies(hwnd: HWND) {
    let replies = crate::overlay::creation_preview::take_async_replies(hwnd);
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            for script in replies {
                let _ = webview.evaluate_script(&script);
            }
        }
    });
}

pub(super) fn flush_replies(hwnd: HWND) {
    let replies = super::ipc_worker::take_replies(hwnd);
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            for script in replies {
                let _ = webview.evaluate_script(&script);
            }
        }
    });
}

fn dispatch_window(hwnd: HWND, cmd: &str) -> Option<Result<Value, String>> {
    Some(match cmd {
        "pick_images" => {
            crate::overlay::creation_file_dialogs::pick_images_dialog(hwnd).map(|paths| {
                Value::Array(
                    paths
                        .into_iter()
                        .map(|path| Value::String(path.to_string_lossy().into_owned()))
                        .collect(),
                )
            })
        }
        "close_window" => {
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
            Ok(Value::Null)
        }
        "minimize_window" => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            Ok(Value::Null)
        }
        "start_drag" => {
            crate::overlay::utils::begin_window_drag(hwnd);
            Ok(Value::Null)
        }
        _ => return None,
    })
}

const HISTORY_HOOKS: ResultPathHooks = ResultPathHooks {
    remap: super::runtime::remap_result_path,
    forget: super::runtime::forget_result_path,
};

pub(super) fn dispatch(
    cmd: &str,
    args: &Value,
    active: &std::sync::atomic::AtomicBool,
) -> Result<Value, CreationError> {
    if let Some(reply) =
        crate::overlay::generation_history::commands::dispatch("3d", cmd, args, &HISTORY_HOOKS)
    {
        return reply;
    }
    Ok(dispatch_command(cmd, args, active)?)
}

fn dispatch_command(
    cmd: &str,
    args: &Value,
    active: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    match cmd {
        "validate_image" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            let mode = args
                .get("generationMode")
                .and_then(Value::as_str)
                .ok_or_else(|| "generationMode is required".to_string())?;
            if !matches!(mode, "quality") {
                return Err("Invalid generation mode".to_string());
            }
            match crate::overlay::creation_source::inspect_image(path) {
                Ok(_) => Ok(json!({"error": Value::Null})),
                Err(_) => Ok(json!({"error": "image_invalid"})),
            }
        }
        "default_output_dir" => Ok(Value::String(
            super::runtime::default_output_dir()?
                .to_string_lossy()
                .to_string(),
        )),
        "export_result" => {
            let output_path = args
                .get("outputPath")
                .and_then(Value::as_str)
                .ok_or_else(|| "outputPath is required".to_string())?;
            serde_json::to_value(super::export::export_result(output_path)?)
                .map_err(|err| err.to_string())
        }
        "start_job" => {
            let request: super::runtime::StartJobRequest =
                serde_json::from_value(args.clone()).map_err(|err| err.to_string())?;
            serde_json::to_value(super::runtime::start_job(request)?).map_err(|err| err.to_string())
        }
        "refine_model" => {
            let request: super::runtime::RefineRequest =
                serde_json::from_value(args.clone()).map_err(|err| err.to_string())?;
            serde_json::to_value(super::runtime::start_refinement(request)?)
                .map_err(|err| err.to_string())
        }
        "prepare_runtime" => Ok(Value::String(super::runtime::prepare_runtime())),
        "capacity_verification_status" => {
            Ok(crate::overlay::creation_runtime::verification::status())
        }
        "refresh_capacity_verification" => {
            crate::overlay::creation_runtime::verification::refresh();
            Ok(Value::Null)
        }
        "verify_capacity" => Ok(crate::overlay::creation_runtime::verification::start()),
        "runtime_preparation_status" => {
            Ok(Value::String(super::runtime::runtime_preparation_status()))
        }
        "generation_capabilities" => Ok(super::runtime::product_capabilities()),
        "cancel_job" => {
            let job_id = args.get("jobId").and_then(Value::as_str);
            serde_json::to_value(super::runtime::cancel_job(job_id)).map_err(|err| err.to_string())
        }
        "job_status" => {
            let job_id = args.get("jobId").and_then(Value::as_str);
            serde_json::to_value(super::runtime::job_status(job_id)).map_err(|err| err.to_string())
        }
        "job_statuses" => {
            serde_json::to_value(super::runtime::job_statuses()).map_err(|err| err.to_string())
        }
        "recovery_pending" => Ok(Value::Bool(super::runtime::recovery_pending())),
        "model_asset_url" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            super::asset_protocol::issue(path, active)
        }
        "release_model_asset" => {
            super::asset_protocol::clear();
            Ok(Value::Null)
        }
        _ => Err(format!("unknown cmd: {cmd}")),
    }
}

fn send_reply(cmd: &str, id: &str, result: Result<Value, CreationError>) {
    if let Some(diagnostic) = failure_diagnostic(cmd, &result) {
        crate::log_info!("{diagnostic}");
    }
    if id.is_empty() {
        return;
    }
    let script = reply_script(id, result);
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}

#[cfg(test)]
#[path = "input_retired_mode_tests.rs"]
mod input_retired_mode_tests;
#[cfg(test)]
#[path = "input_validation_tests.rs"]
mod input_validation_tests;
