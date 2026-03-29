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
        "add_hotkey" => handle_add_hotkey(envelope.args),
        "remove_hotkey" => handle_remove_hotkey(envelope.args),
        "open_tts_settings" => {
            // Minimize the relay window
            unsafe {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            // Show badge hint
            let lang = super::current_ui_language();
            let locale = crate::gui::locale::LocaleText::get(&lang);
            crate::overlay::auto_copy_badge::show_notification(
                locale.bilingual_relay_tts_settings_hint,
            );
            // Dismiss splash if still showing, then open TTS modal
            super::REQUEST_DISMISS_SPLASH.store(true, std::sync::atomic::Ordering::SeqCst);
            super::REQUEST_OPEN_TTS_SETTINGS.store(true, std::sync::atomic::Ordering::SeqCst);
            crate::gui::signal_restore_window();
            if let Ok(ctx) = crate::gui::GUI_CONTEXT.lock() {
                if let Some(ctx) = ctx.as_ref() {
                    ctx.request_repaint();
                }
            }
            Ok(Value::Null)
        }
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

/// Add hotkey immediately — saves to config and triggers system-wide registration.
fn handle_add_hotkey(args: Value) -> Result<Value, String> {
    let args = serde_json::from_value::<HotkeyArgs>(args).map_err(|err| err.to_string())?;
    let Some(mut hotkey) = super::map_hotkey(&args.key, &args.code, args.ctrl, args.alt, args.shift, args.meta) else {
        return Err("unsupported key".to_string());
    };

    // Check conflicts
    {
        let app = crate::APP.lock().unwrap();
        if let Some(msg) = app.config.check_hotkey_conflict(hotkey.code, hotkey.modifiers, None) {
            return Err(msg);
        }
    }

    hotkey.name = super::hotkey_label(hotkey.modifiers, &hotkey.name);

    // Save immediately to config
    {
        let mut app = crate::APP.lock().unwrap();
        app.config.bilingual_relay.hotkeys.push(hotkey.clone());
        crate::config::save_config(&app.config);
    }

    // Trigger system-wide hotkey re-registration
    super::reload_hotkeys();

    // Update UI state
    super::state::with_state(|ui| {
        ui.draft.hotkeys = crate::APP.lock().unwrap().config.bilingual_relay.hotkeys.clone();
        ui.applied.hotkeys = ui.draft.hotkeys.clone();
        ui.hotkey_error = None;
        ui.normalize();
    });
    super::state::request_sync();

    Ok(serde_json::to_value(&hotkey).unwrap_or(Value::Null))
}

/// Remove hotkey by index immediately.
fn handle_remove_hotkey(args: Value) -> Result<Value, String> {
    let index = args
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or("missing index")? as usize;

    {
        let mut app = crate::APP.lock().unwrap();
        if index < app.config.bilingual_relay.hotkeys.len() {
            app.config.bilingual_relay.hotkeys.remove(index);
            crate::config::save_config(&app.config);
        }
    }

    super::reload_hotkeys();

    super::state::with_state(|ui| {
        ui.draft.hotkeys = crate::APP.lock().unwrap().config.bilingual_relay.hotkeys.clone();
        ui.applied.hotkeys = ui.draft.hotkeys.clone();
        ui.normalize();
    });
    super::state::request_sync();

    Ok(Value::Null)
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
