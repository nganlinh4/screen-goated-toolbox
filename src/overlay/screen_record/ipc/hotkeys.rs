// --- HOTKEY MANAGEMENT ---
// Recorder hotkey bindings, JS key-code to VK mapping, and pausing the host's
// global hotkeys while a binding is recorded. Every change goes through the
// host that spawned this worker, never a global window lookup.

use sgt_recorder_protocol::{RecorderHotkey, SettingsChange};
use std::sync::{LazyLock, Mutex};

use crate::hotkey::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

static SHOWN_HOTKEYS: LazyLock<Mutex<Vec<RecorderHotkey>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub(super) fn handle_get_hotkeys() -> Result<serde_json::Value, String> {
    let settings = super::super::worker_settings::request(SettingsChange::Snapshot)?;
    let displayed: Vec<_> = settings
        .hotkeys
        .iter()
        .map(|h| crate::config::Hotkey::new(h.code, h.modifiers))
        .collect();
    *SHOWN_HOTKEYS.lock().unwrap() = settings.hotkeys;
    Ok(serde_json::to_value(displayed).unwrap())
}

pub(super) fn handle_remove_hotkey(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let index = args["index"].as_u64().ok_or("Missing index")? as usize;
    let binding = SHOWN_HOTKEYS.lock().unwrap().get(index).cloned();
    if let Some(binding) = binding {
        super::super::worker_settings::request(SettingsChange::RemoveHotkey(binding.clone()))?;
        let mut shown = SHOWN_HOTKEYS.lock().unwrap();
        if shown.get(index) == Some(&binding) {
            shown.remove(index);
        }
    }
    Ok(serde_json::Value::Null)
}

pub(super) fn handle_set_hotkey(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let code_str = args["code"].as_str().ok_or("Missing code")?;
    let mods_arr = args["modifiers"].as_array().ok_or("Missing modifiers")?;
    let key = args["key"].as_str().unwrap_or("");

    let mut modifiers = 0;
    for m in mods_arr {
        match m.as_str() {
            Some("Control") => modifiers |= MOD_CONTROL,
            Some("Alt") => modifiers |= MOD_ALT,
            Some("Shift") => modifiers |= MOD_SHIFT,
            Some("Meta") => modifiers |= MOD_WIN,
            _ => {}
        }
    }

    let hotkey = crate::hotkey::web_binding::from_web_event(
        key,
        code_str,
        modifiers & MOD_CONTROL != 0,
        modifiers & MOD_ALT != 0,
        modifiers & MOD_SHIFT != 0,
        modifiers & MOD_WIN != 0,
    )
    .ok_or_else(|| format!("Unsupported key code: {code_str}"))?;

    let binding = RecorderHotkey {
        code: hotkey.code,
        modifiers: hotkey.modifiers,
    };
    super::super::worker_settings::request(SettingsChange::AddHotkey(binding.clone()))?;
    SHOWN_HOTKEYS.lock().unwrap().push(binding);

    Ok(serde_json::to_value(&hotkey).unwrap())
}

pub(super) fn handle_unregister_hotkeys() -> Result<serde_json::Value, String> {
    super::super::worker_settings::request(SettingsChange::BindingCapture(true))?;
    Ok(serde_json::Value::Null)
}

pub(super) fn handle_register_hotkeys() -> Result<serde_json::Value, String> {
    super::super::worker_settings::request(SettingsChange::BindingCapture(false))?;
    Ok(serde_json::Value::Null)
}
