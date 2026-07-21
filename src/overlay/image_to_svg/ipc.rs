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
        Ok(envelope) => envelope,
        Err(error) => {
            eprintln!("[image-to-svg] invalid ipc: {error}");
            return;
        }
    };
    send_reply(&envelope.id, dispatch(hwnd, &envelope.cmd, &envelope.args));
}

fn dispatch(hwnd: HWND, cmd: &str, args: &Value) -> Result<Value, String> {
    match cmd {
        "pick_images" => {
            crate::overlay::three_d_generator::file_dialogs::pick_images_dialog().map(|paths| {
                Value::Array(
                    paths
                        .into_iter()
                        .map(|path| Value::String(path.to_string_lossy().to_string()))
                        .collect(),
                )
            })
        }
        "pick_output_dir" => {
            crate::overlay::three_d_generator::file_dialogs::pick_output_dir_dialog().map(|path| {
                path.map(|value| Value::String(value.to_string_lossy().to_string()))
                    .unwrap_or(Value::Null)
            })
        }
        "default_output_dir" => Ok(Value::String(
            super::runtime::default_output_dir()
                .to_string_lossy()
                .to_string(),
        )),
        "start_job" => {
            let request: super::runtime::StartJobRequest =
                serde_json::from_value(args.clone()).map_err(|error| error.to_string())?;
            serde_json::to_value(super::runtime::start_job(request)?)
                .map_err(|error| error.to_string())
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
        "history_results" => serde_json::to_value(crate::overlay::generation_history::list("svg")?)
            .map_err(|error| error.to_string()),
        "rename_history_result" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "id is required".to_string())?;
            let new_name = args
                .get("newName")
                .and_then(Value::as_str)
                .ok_or_else(|| "newName is required".to_string())?;
            let previous = crate::overlay::generation_history::list("svg")?
                .into_iter()
                .find(|entry| entry.id == id)
                .ok_or_else(|| "Result is no longer in history.".to_string())?;
            let updated = crate::overlay::generation_history::rename("svg", id, new_name)?;
            super::runtime::remap_result_path(&previous.output_path, &updated.output_path);
            serde_json::to_value(updated).map_err(|error| error.to_string())
        }
        "delete_history_result" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "id is required".to_string())?;
            let previous = crate::overlay::generation_history::list("svg")?
                .into_iter()
                .find(|entry| entry.id == id)
                .ok_or_else(|| "Result is no longer in history.".to_string())?;
            crate::overlay::generation_history::delete("svg", id)?;
            super::runtime::forget_result_path(&previous.output_path);
            Ok(Value::Null)
        }
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

fn send_reply(id: &str, result: Result<Value, String>) {
    if id.is_empty() {
        return;
    }
    let payload = match result {
        Ok(value) => json!({ "id": id, "result": value }),
        Err(error) => json!({ "id": id, "error": error }),
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));");
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
