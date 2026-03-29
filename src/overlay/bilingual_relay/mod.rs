mod assets;
mod ipc;
mod runtime;
mod state;
mod window;

use std::sync::Once;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;
use wry::WebContext;

use crate::config::{BilingualRelaySettings, Hotkey, save_config};
use crate::gui::locale::LocaleText;
use crate::win_types::SendHwnd;

pub use runtime::{RelayConnectionState, RelayTranscriptItem};

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 321;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 322;

pub(super) static REGISTER_CLASS: Once = Once::new();
pub(super) static mut WINDOW_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
pub(super) static mut IS_READY: bool = false;
pub(super) static mut IS_INITIALIZING: bool = false;

thread_local! {
    pub(super) static WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> = const { std::cell::RefCell::new(None) };
}

pub fn show_bilingual_relay() {
    window::show();
}

pub fn update_settings() {
    state::refresh_from_config();
    state::request_sync();
}

pub(super) fn clear_transcripts() {
    state::clear_transcripts();
    state::request_sync();
}

pub(super) fn publish_connection(
    connection_state: RelayConnectionState,
    is_running: bool,
    last_error: Option<String>,
) {
    state::publish_connection(connection_state, is_running, last_error);
    state::request_sync();
}

pub(super) fn publish_error(
    connection_state: RelayConnectionState,
    error: String,
    is_running: bool,
) {
    state::publish_error(connection_state, error, is_running);
    state::request_sync();
}

pub(super) fn publish_audio_level(level: f32) {
    state::publish_audio_level(level);
    state::request_sync();
}

pub(super) fn upsert_transcript(role: &'static str, text: String, is_final: bool) {
    state::upsert_transcript(role, text, is_final);
    state::request_sync();
}

pub(super) fn finalize_transcripts() {
    state::finalize_transcripts();
    state::request_sync();
}

pub(super) fn auto_start_if_possible() {
    let applied = current_settings();
    if applied.is_valid() {
        start_if_possible(applied);
    } else {
        publish_connection(RelayConnectionState::NotConfigured, false, None);
    }
}

pub(super) fn start_if_possible(settings: BilingualRelaySettings) {
    let locale = LocaleText::get(&current_ui_language());
    let api_key_missing = crate::APP
        .lock()
        .map(|app| app.config.gemini_api_key.trim().is_empty())
        .unwrap_or(true);
    if api_key_missing {
        publish_error(
            RelayConnectionState::Error,
            locale.bilingual_relay_api_key_required.to_string(),
            false,
        );
        return;
    }

    unsafe {
        let hwnd = std::ptr::addr_of!(WINDOW_HWND).read();
        if !hwnd.is_invalid() {
            runtime::start_session(hwnd.as_isize(), settings);
        }
    }
}

pub(super) fn apply_draft() {
    let (draft, can_apply) = state::with_state(|ui| {
        ui.normalize();
        (ui.draft.clone(), ui.draft.is_valid())
    });

    if !can_apply {
        publish_connection(RelayConnectionState::NotConfigured, false, None);
        return;
    }

    {
        let mut app = crate::APP.lock().unwrap();
        app.config.bilingual_relay = draft.clone();
        save_config(&app.config);
    }

    state::with_state(|ui| {
        ui.applied = draft.clone();
        ui.draft = draft.clone();
        ui.transcripts.clear();
        ui.last_error = None;
        ui.hotkey_error = None;
        ui.audio_level = 0.0;
        ui.normalize();
    });

    reload_hotkeys();
    state::request_sync();
    start_if_possible(draft);
}

pub(super) fn toggle_run() {
    let snapshot = state::snapshot();
    if snapshot.is_running {
        runtime::stop_session();
        publish_connection(RelayConnectionState::Stopped, false, None);
        return;
    }
    if snapshot.applied.is_valid() {
        start_if_possible(snapshot.applied);
    } else {
        publish_connection(RelayConnectionState::NotConfigured, false, None);
    }
}

pub(super) fn apply_hotkey_capture(
    key: &str,
    code: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) {
    let Some(mut hotkey) = map_hotkey(key, code, ctrl, alt, shift, meta) else {
        return;
    };

    let conflict = {
        let app = crate::APP.lock().unwrap();
        if app
            .config
            .bilingual_relay
            .hotkey
            .as_ref()
            .map(|existing| existing.code == hotkey.code && existing.modifiers == hotkey.modifiers)
            .unwrap_or(false)
        {
            None
        } else {
            app.config
                .check_hotkey_conflict(hotkey.code, hotkey.modifiers, None)
        }
    };

    state::with_state(|ui| {
        if conflict.is_some() {
            let locale = LocaleText::get(&current_ui_language());
            ui.hotkey_error = Some(locale.bilingual_relay_hotkey_conflict.to_string());
        } else {
            hotkey.name = hotkey_label(hotkey.modifiers, &hotkey.name);
            ui.draft.hotkey = Some(hotkey);
            ui.hotkey_error = None;
            ui.last_error = None;
        }
        ui.normalize();
    });

    state::request_sync();
}

pub(super) fn current_settings() -> BilingualRelaySettings {
    crate::APP
        .lock()
        .map(|app| app.config.bilingual_relay.clone().normalized())
        .unwrap_or_default()
}

pub(super) fn current_ui_language() -> String {
    crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string())
}

pub(super) fn connection_key(connection_state: RelayConnectionState) -> &'static str {
    match connection_state {
        RelayConnectionState::NotConfigured => "not_configured",
        RelayConnectionState::Connecting => "connecting",
        RelayConnectionState::Ready => "ready",
        RelayConnectionState::Reconnecting => "reconnecting",
        RelayConnectionState::Error => "error",
        RelayConnectionState::Stopped => "stopped",
    }
}

pub(super) fn status_label(
    text: &LocaleText,
    connection_state: RelayConnectionState,
) -> &'static str {
    match connection_state {
        RelayConnectionState::NotConfigured => text.bilingual_relay_status_not_configured,
        RelayConnectionState::Connecting => text.bilingual_relay_status_connecting,
        RelayConnectionState::Ready => text.bilingual_relay_status_ready,
        RelayConnectionState::Reconnecting => text.bilingual_relay_status_reconnecting,
        RelayConnectionState::Error => text.bilingual_relay_status_error,
        RelayConnectionState::Stopped => text.bilingual_relay_status_stopped,
    }
}

fn reload_hotkeys() {
    unsafe {
        let listener = FindWindowW(w!("HotkeyListenerClass"), w!("Listener")).unwrap_or_default();
        if !listener.is_invalid() {
            let _ = PostMessageW(
                Some(listener),
                crate::hotkey::WM_RELOAD_HOTKEYS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn map_hotkey(
    key: &str,
    code: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> Option<Hotkey> {
    let key_name = normalize_key_name(key, code)?;
    let vk = map_virtual_key(code, key)?;
    let modifiers = (if ctrl { MOD_CONTROL } else { 0 })
        | (if alt { MOD_ALT } else { 0 })
        | (if shift { MOD_SHIFT } else { 0 })
        | (if meta { MOD_WIN } else { 0 });

    Some(Hotkey {
        code: vk,
        name: key_name,
        modifiers,
    })
}

fn hotkey_label(modifiers: u32, key_name: &str) -> String {
    let mut parts = Vec::new();
    if modifiers & MOD_CONTROL != 0 {
        parts.push("Ctrl");
    }
    if modifiers & MOD_ALT != 0 {
        parts.push("Alt");
    }
    if modifiers & MOD_SHIFT != 0 {
        parts.push("Shift");
    }
    if modifiers & MOD_WIN != 0 {
        parts.push("Win");
    }
    parts.push(key_name);
    parts.join("+")
}

fn normalize_key_name(key: &str, code: &str) -> Option<String> {
    let code = code.trim();
    if let Some(letter) = code.strip_prefix("Key") {
        return Some(letter.to_string());
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return Some(digit.to_string());
    }
    if let Some(function) = code.strip_prefix('F') {
        if function.parse::<u8>().is_ok() {
            return Some(code.to_string());
        }
    }

    match code {
        "Space" => Some("Space".to_string()),
        "Minus" => Some("-".to_string()),
        "Equal" => Some("=".to_string()),
        "BracketLeft" => Some("[".to_string()),
        "BracketRight" => Some("]".to_string()),
        "Backslash" => Some("\\".to_string()),
        "Semicolon" => Some(";".to_string()),
        "Quote" => Some("'".to_string()),
        "Comma" => Some(",".to_string()),
        "Period" => Some(".".to_string()),
        "Slash" => Some("/".to_string()),
        "Backquote" => Some("`".to_string()),
        "Escape" => Some("Esc".to_string()),
        "Tab" => Some("Tab".to_string()),
        "Enter" => Some("Enter".to_string()),
        "ArrowUp" => Some("Up".to_string()),
        "ArrowDown" => Some("Down".to_string()),
        "ArrowLeft" => Some("Left".to_string()),
        "ArrowRight" => Some("Right".to_string()),
        "Insert" => Some("Insert".to_string()),
        "Delete" => Some("Delete".to_string()),
        "Home" => Some("Home".to_string()),
        "End" => Some("End".to_string()),
        "PageUp" => Some("PageUp".to_string()),
        "PageDown" => Some("PageDown".to_string()),
        _ => match key {
            "Control" | "Shift" | "Alt" | "Meta" => None,
            _ if key.len() == 1 => Some(key.to_uppercase()),
            _ => None,
        },
    }
}

fn map_virtual_key(code: &str, key: &str) -> Option<u32> {
    if let Some(letter) = code.strip_prefix("Key") {
        return letter.as_bytes().first().copied().map(u32::from);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.as_bytes().first().copied().map(u32::from);
    }
    if let Some(number) = code.strip_prefix('F')
        && let Ok(index) = number.parse::<u32>()
    {
        return Some(111 + index);
    }

    Some(match code {
        "Space" => 0x20,
        "Tab" => 0x09,
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        _ => {
            return normalize_key_name(key, code)
                .and_then(|name| name.as_bytes().first().copied())
                .map(u32::from);
        }
    })
}
