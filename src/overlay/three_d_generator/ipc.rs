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
    let reply = dispatch(hwnd, &envelope.cmd, &envelope.args);
    send_reply(&envelope.id, reply);
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
            super::runtime::default_output_dir()
                .to_string_lossy()
                .to_string(),
        )),
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
        "prepare_runtime" => Ok(Value::String(super::runtime::prepare_runtime())),
        "runtime_preparation_status" => {
            Ok(Value::String(super::runtime::runtime_preparation_status()))
        }
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
        "read_asset" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "path is required".to_string())?;
            super::runtime::read_asset(path)
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
        Err(err) => json!({ "id": id, "error": err }),
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));");
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
