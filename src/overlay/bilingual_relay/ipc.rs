use serde::Deserialize;
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    HTCAPTION, PostMessageW, SW_HIDE, SW_MINIMIZE, SendMessageW, ShowWindow, WM_CLOSE,
    WM_NCLBUTTONDOWN,
};

#[derive(Deserialize)]
struct IpcEnvelope {
    id: String,
    cmd: String,
    #[serde(default)]
    args: Value,
}

#[derive(Deserialize)]
struct DraftArgs {
    profile: String,
    field: String,
    value: String,
}

#[derive(Deserialize)]
struct HotkeyArgs {
    key: String,
    code: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

pub(super) fn handle_ipc(hwnd: HWND, body: &str) {
    let envelope = match serde_json::from_str::<IpcEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(err) => {
            super::publish_error(
                super::RelayConnectionState::Error,
                format!("invalid ipc payload: {err}"),
                false,
            );
            return;
        }
    };

    let result = match envelope.cmd.as_str() {
        "set_draft" => handle_set_draft(envelope.args).map(|_| Value::Null),
        "apply" => {
            super::apply_draft();
            Ok(Value::Null)
        }
        "toggle_run" => {
            super::toggle_run();
            Ok(Value::Null)
        }
        "clear_hotkey" => {
            super::state::with_state(|ui| {
                ui.draft.hotkey = None;
                ui.hotkey_error = None;
                ui.normalize();
            });
            super::state::request_sync();
            Ok(Value::Null)
        }
        "set_hotkey" => handle_set_hotkey(envelope.args).map(|_| Value::Null),
        "drag_window" => {
            unsafe {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
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
        "close_window" => {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            Ok(Value::Null)
        }
        other => Err(format!("unknown ipc command: {other}")),
    };

    match result {
        Ok(value) => reply_to_webview(&envelope.id, json!({ "id": envelope.id, "result": value })),
        Err(error) => reply_to_webview(
            &envelope.id,
            json!({ "id": envelope.id, "error": error, "result": Value::Null }),
        ),
    }
}

fn handle_set_draft(args: Value) -> Result<(), String> {
    let args = serde_json::from_value::<DraftArgs>(args).map_err(|err| err.to_string())?;
    super::state::with_state(|ui| {
        let target = if args.profile == "first" {
            &mut ui.draft.first
        } else {
            &mut ui.draft.second
        };
        match args.field.as_str() {
            "language" => target.language = args.value,
            "accent" => target.accent = args.value,
            "tone" => target.tone = args.value,
            _ => {}
        }
        ui.last_error = None;
        ui.hotkey_error = None;
        ui.normalize();
    });
    super::state::request_sync();
    Ok(())
}

fn handle_set_hotkey(args: Value) -> Result<(), String> {
    let args = serde_json::from_value::<HotkeyArgs>(args).map_err(|err| err.to_string())?;
    super::apply_hotkey_capture(
        &args.key, &args.code, args.ctrl, args.alt, args.shift, args.meta,
    );
    Ok(())
}

fn reply_to_webview(_id: &str, payload: Value) {
    let Ok(payload_json) = serde_json::to_string(&payload) else {
        return;
    };
    let script = format!(
        "window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload_json} }}));"
    );
    super::WEBVIEW.with(|webview| {
        if let Some(webview) = webview.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
