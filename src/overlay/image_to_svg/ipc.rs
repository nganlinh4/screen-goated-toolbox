use crate::overlay::creation_commands::{self, Envelope as IpcEnvelope};
use crate::overlay::creation_error::CreationError;
use crate::overlay::generation_history::commands::ResultPathHooks;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_MINIMIZE, ShowWindow, WM_CLOSE};

pub(super) fn handle_ipc(hwnd: HWND, body: &str) {
    let envelope: IpcEnvelope = match serde_json::from_str(body) {
        Ok(envelope) => envelope,
        Err(error) => {
            eprintln!("[image-to-svg] invalid ipc: {error}");
            return;
        }
    };
    if envelope.cmd == "start_job" {
        let id = envelope.id.clone();
        if let Err(error) = creation_commands::request(hwnd, envelope, body.len()) {
            send_reply(&id, Err(error));
        }
        return;
    }
    if envelope.cmd == "read_image_preview" {
        if let Err(error) = request_image_preview(hwnd, &envelope) {
            send_reply(&envelope.id, Err(error.into()));
        }
        return;
    }
    let reply = crate::overlay::generation_history::commands::dispatch(
        "svg",
        &envelope.cmd,
        &envelope.args,
        &HISTORY_HOOKS,
    )
    .unwrap_or_else(|| Ok(dispatch(hwnd, &envelope.cmd, &envelope.args)?));
    if let Err(error) = &reply {
        crate::log_info!("[image-to-svg] {} failed: {error}", envelope.cmd);
    }
    send_reply(&envelope.id, reply);
}

const HISTORY_HOOKS: ResultPathHooks = ResultPathHooks {
    remap: super::runtime::remap_result_path,
    forget: super::runtime::forget_result_path,
};

pub(super) fn register_commands(hwnd: HWND) {
    creation_commands::register(
        hwnd,
        super::WM_APP_IPC_REPLY,
        1024 * 1024,
        dispatch_submission,
    );
}

fn dispatch_submission(
    cmd: &str,
    args: &Value,
    active: &AtomicBool,
) -> Result<Value, CreationError> {
    if cmd != "start_job" || !active.load(Ordering::Acquire) {
        return Err("Creation submission is no longer available.".into());
    }

    let request: super::runtime::StartJobRequest =
        serde_json::from_value(args.clone()).map_err(|error| error.to_string())?;
    serde_json::to_value(super::runtime::start_job(request)?)
        .map_err(|error| error.to_string().into())
}

pub(super) fn flush_replies(hwnd: HWND) {
    let replies = creation_commands::take_replies(hwnd);
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            for script in replies {
                let _ = webview.evaluate_script(&script);
            }
        }
    });
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

fn dispatch(hwnd: HWND, cmd: &str, args: &Value) -> Result<Value, String> {
    match cmd {
        "pick_images" => {
            crate::overlay::creation_file_dialogs::pick_images_dialog(hwnd).map(|paths| {
                Value::Array(
                    paths
                        .into_iter()
                        .map(|path| Value::String(path.to_string_lossy().to_string()))
                        .collect(),
                )
            })
        }
        "pick_output_dir" => crate::overlay::creation_file_dialogs::pick_output_dir_dialog(hwnd)
            .map(|path| {
                path.map(|value| Value::String(value.to_string_lossy().to_string()))
                    .unwrap_or(Value::Null)
            }),
        "default_output_dir" => Ok(Value::String(
            super::runtime::default_output_dir()?
                .to_string_lossy()
                .to_string(),
        )),
        "image_asset_url" => {
            crate::overlay::creation_preview_protocol::issue_source_image_from_args(args)
        }
        "svg_asset_url" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            super::runtime::issue_static_asset(path)
        }
        "validate_image" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            Ok(json!({"error": super::input_policy::error(std::path::Path::new(path))}))
        }

        "prepare_runtime" => Ok(Value::String(super::runtime::prepare_runtime())),
        "runtime_preparation_status" => {
            Ok(Value::String(super::runtime::runtime_preparation_status()))
        }
        "cancel_job" => {
            let job_id = args.get("jobId").and_then(Value::as_str);
            serde_json::to_value(super::runtime::cancel_job(job_id))
                .map_err(|error| error.to_string())
        }
        "job_statuses" => {
            serde_json::to_value(super::runtime::job_statuses()).map_err(|error| error.to_string())
        }
        "recovery_pending" => Ok(Value::Bool(super::runtime::recovery_pending())),
        "read_asset" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            super::runtime::read_asset(path)
        }
        "save_svg_edits" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            let svg = args
                .get("svg")
                .and_then(Value::as_str)
                .ok_or_else(|| "svg is required".to_string())?;
            super::runtime::save_svg_edits(path, svg)
        }
        "open_output" => {
            let path = args.get("path").and_then(Value::as_str);
            super::runtime::open_output(path)?;
            Ok(Value::Null)
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
        _ => Err(format!("unknown cmd: {cmd}")),
    }
}

fn send_reply(id: &str, result: Result<Value, CreationError>) {
    if id.is_empty() {
        return;
    }
    let script = creation_commands::reply_script(id, result);
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
