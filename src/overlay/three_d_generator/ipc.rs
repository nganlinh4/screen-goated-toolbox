use serde::Deserialize;
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_MINIMIZE, ShowWindow, WM_CLOSE};

#[derive(Deserialize)]
struct IpcEnvelope {
    #[serde(default)]
    id: String,
    cmd: String,
    #[serde(default)]
    args: Value,
}

pub(super) fn handle_ipc(hwnd: HWND, body: &str) {
    let envelope: IpcEnvelope = match serde_json::from_str(body) {
        Ok(env) => env,
        Err(err) => {
            eprintln!("[3d-generator] invalid ipc: {err}");
            return;
        }
    };
    if envelope.cmd == "read_image_preview" {
        if let Err(error) = request_image_preview(hwnd, &envelope) {
            send_reply(&envelope.id, Err(error));
        }
        return;
    }
    let reply = dispatch(hwnd, &envelope.cmd, &envelope.args);
    if let Err(error) = &reply {
        eprintln!("[3d-generator] {} failed: {error}", envelope.cmd);
    }
    send_reply(&envelope.id, reply);
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
        "pick_image" => super::file_dialogs::pick_image_dialog().map(|opt| {
            opt.map(|path| Value::String(path.to_string_lossy().to_string()))
                .unwrap_or(Value::Null)
        }),
        "pick_images" => super::file_dialogs::pick_images_dialog().map(|paths| {
            Value::Array(
                paths
                    .into_iter()
                    .map(|path| Value::String(path.to_string_lossy().to_string()))
                    .collect(),
            )
        }),
        "pick_output_dir" => super::file_dialogs::pick_output_dir_dialog().map(|opt| {
            opt.map(|path| Value::String(path.to_string_lossy().to_string()))
                .unwrap_or(Value::Null)
        }),
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
        "segment_model" => {
            let continuation_id = args
                .get("continuationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "continuationId is required".to_string())?;
            serde_json::to_value(super::runtime::start_segmentation(continuation_id)?)
                .map_err(|err| err.to_string())
        }
        "refine_model" => {
            let request: super::runtime::RefineRequest =
                serde_json::from_value(args.clone()).map_err(|err| err.to_string())?;
            serde_json::to_value(super::runtime::start_refinement(request)?)
                .map_err(|err| err.to_string())
        }
        "prepare_runtime" => Ok(Value::String(super::runtime::prepare_runtime())),
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
        "history_results" => {
            let entries = crate::overlay::generation_history::list("3d")?;
            serde_json::to_value(crate::overlay::generation_history::public_entries(&entries))
                .map_err(|err| err.to_string())
        }
        "rename_history_result" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "id is required".to_string())?;
            let new_name = args
                .get("newName")
                .and_then(Value::as_str)
                .ok_or_else(|| "newName is required".to_string())?;
            let previous = crate::overlay::generation_history::list("3d")?
                .into_iter()
                .find(|entry| entry.id == id)
                .ok_or_else(|| "Result is no longer in history.".to_string())?;
            let updated = crate::overlay::generation_history::rename("3d", id, new_name)?;
            super::runtime::remap_result_path(&previous.output_path, &updated.output_path);
            serde_json::to_value(crate::overlay::generation_history::public_entry(&updated))
                .map_err(|err| err.to_string())
        }
        "delete_history_result" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "id is required".to_string())?;
            let previous = crate::overlay::generation_history::list("3d")?
                .into_iter()
                .find(|entry| entry.id == id)
                .ok_or_else(|| "Result is no longer in history.".to_string())?;
            crate::overlay::generation_history::delete("3d", id)?;
            super::runtime::forget_result_path(&previous.output_path);
            Ok(Value::Null)
        }
        "delete_all_history_results" => {
            let previous = crate::overlay::generation_history::list("3d")?;
            let deleted = crate::overlay::generation_history::delete_all("3d")?;
            for entry in previous {
                super::runtime::forget_result_path(&entry.output_path);
            }
            Ok(Value::from(deleted as u64))
        }
        "model_asset_url" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            super::asset_protocol::issue(path)
        }
        "release_model_asset" => {
            super::asset_protocol::clear();
            Ok(Value::Null)
        }
        "open_output" => {
            let kind = args.get("kind").and_then(Value::as_str).unwrap_or("folder");
            let path = args.get("path").and_then(Value::as_str);
            super::runtime::open_output(kind, path)?;
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

fn send_reply(id: &str, result: Result<Value, String>) {
    if id.is_empty() {
        return;
    }
    let payload = match result {
        Ok(value) => json!({ "id": id, "result": value }),
        Err(_error) => json!({ "id": id, "error": "operation_failed" }),
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));");
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
