//! Continuous Mode State Management
//!
//! This module handles the "hold-to-activate continuous mode" feature for image and text presets.
//! When activated, the preset will automatically retrigger after each completion.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Whether continuous mode is currently active
static CONTINUOUS_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether continuous mode is pending start (e.g. from favorite bubble)
static CONTINUOUS_PENDING_START: AtomicBool = AtomicBool::new(false);

/// The preset index that is running in continuous mode
static CONTINUOUS_PRESET_IDX: AtomicUsize = AtomicUsize::new(0);

/// The hotkey name to display in the exit message (e.g., "Ctrl+Shift+T")
static CONTINUOUS_HOTKEY_NAME: Mutex<String> = Mutex::new(String::new());

/// The name of the latest hotkey that triggered an action (used for "Hold" detection logic finding the name)
static LATEST_HOTKEY_NAME: Mutex<String> = Mutex::new(String::new());

/// Check if continuous mode is currently active
pub fn is_active() -> bool {
    CONTINUOUS_MODE_ACTIVE.load(Ordering::SeqCst)
}

/// Check if continuous mode is pending start
pub fn is_pending_start() -> bool {
    CONTINUOUS_PENDING_START.load(Ordering::SeqCst)
}

/// Set pending start for a preset
pub fn set_pending_start(preset_idx: usize, hotkey_name: String) {
    CONTINUOUS_PRESET_IDX.store(preset_idx, Ordering::SeqCst);
    *CONTINUOUS_HOTKEY_NAME.lock().unwrap() = hotkey_name;
    CONTINUOUS_PENDING_START.store(true, Ordering::SeqCst);
}

/// Get the preset index running in continuous mode
pub fn get_preset_idx() -> usize {
    CONTINUOUS_PRESET_IDX.load(Ordering::SeqCst)
}

/// Get the hotkey name for the exit message
pub fn get_hotkey_name() -> String {
    CONTINUOUS_HOTKEY_NAME.lock().unwrap().clone()
}

/// Set the latest hotkey name (called by main loop)
pub fn set_latest_hotkey_name(name: String) {
    crate::log_info!("[Continuous] Setting Latest Hotkey Name: '{}'", name);
    *LATEST_HOTKEY_NAME.lock().unwrap() = name;
}

/// Get the latest hotkey name
pub fn get_latest_hotkey_name() -> String {
    LATEST_HOTKEY_NAME.lock().unwrap().clone()
}

/// Activate continuous mode for a preset (promotes pending to active)
pub fn activate(preset_idx: usize, hotkey_name: String) {
    CONTINUOUS_PRESET_IDX.store(preset_idx, Ordering::SeqCst);
    *CONTINUOUS_HOTKEY_NAME.lock().unwrap() = hotkey_name;
    CONTINUOUS_MODE_ACTIVE.store(true, Ordering::SeqCst);
    CONTINUOUS_PENDING_START.store(false, Ordering::SeqCst);
}

/// Deactivate continuous mode
pub fn deactivate() {
    CONTINUOUS_MODE_ACTIVE.store(false, Ordering::SeqCst);
    CONTINUOUS_PENDING_START.store(false, Ordering::SeqCst);
    CONTINUOUS_PRESET_IDX.store(0, Ordering::SeqCst);
    *CONTINUOUS_HOTKEY_NAME.lock().unwrap() = String::new();
}

/// Show the continuous mode activation notification
/// Show the continuous mode activation notification
pub fn show_activation_notification(preset_id: &str, hotkey_name: &str) {
    let lang = {
        if let Ok(app) = crate::APP.lock() {
            app.config.ui_language.clone()
        } else {
            "en".to_string()
        }
    };

    crate::log_info!(
        "[Continuous] Notification Request - Preset: {}, Hotkey: '{}'",
        preset_id,
        hotkey_name
    );

    let localized_name = crate::gui::settings_ui::get_localized_preset_name(preset_id, &lang);

    // 1. Title Suffix
    let suffix = match lang.as_str() {
        "vi" => "Chế độ liên tục",
        "ko" => "연속 모드",
        _ => "Continuous Mode",
    };
    let title = format!("{} - {}", localized_name, suffix);

    // 2. Prepare message from locale
    let locale = crate::gui::locale::LocaleText::get(&lang);
    let mut message = locale.continuous_mode_activated.to_string();

    // Remove Sparkle (User requested to remove sparkle icon from text)
    message = message.replace("✨ ", "").replace("✨", "");

    // Remove Preset Name part (because it's in title now)
    message = message
        .replace("\"{preset}\"", "")
        .replace("'{preset}'", "")
        .replace("{preset}", "");

    // 3. Hotkey Logic
    // If triggered by UI (Bubble), hotkey_name is typically empty or generic "Hotkey"
    // In that case, we want "... press ESC [ ] to exit" (removing the "or choice")
    if hotkey_name.is_empty()
        || hotkey_name.to_lowercase() == "hotkey"
        || hotkey_name.to_lowercase() == "esc"
    {
        // Remove " or {hotkey}" variants
        message = message
            .replace(" hay {hotkey}", "")
            .replace(" or {hotkey}", "")
            .replace(" 또는 {hotkey}", "");

        // Final cleanup for remaining {hotkey} if the structure was different
        message = message.replace("{hotkey}", "");
    } else {
        // Specific Hotkey - keep the structure
        message = message.replace("{hotkey}", hotkey_name);
    }

    // Clean up any double spaces introduced by removals
    loop {
        let new_msg = message.replace("  ", " ");
        if new_msg == message {
            break;
        }
        message = new_msg;
    }
    let message = message.trim();

    // Call the detailed notification
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &title,
        message,
        crate::overlay::auto_copy_badge::NotificationType::Update,
    );
}

/// Check if a preset type supports continuous mode (only image and text)
pub fn supports_continuous_mode(preset_type: &str) -> bool {
    preset_type == "image" || preset_type == "text"
}

// =============================================================================
// HOLD DETECTION STATE
// These are used to track when a hotkey is being held down for continuous mode
// =============================================================================

use std::time::Instant;

/// The hotkey that triggered the current action (for checking if still held)
/// The hotkey that triggered the current action (for checking if still held)
static CURRENT_HOTKEY: Mutex<Option<(u32, u32)>> = Mutex::new(None); // (modifiers, vk_code)

/// Timestamp of the last hotkey trigger attempt (used for heartbeat hold detection)
static LAST_HOTKEY_TRIGGER_TIME: Mutex<Option<Instant>> = Mutex::new(None);
static HEARTBEAT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Reset the heartbeat count for a new session
pub fn reset_heartbeat() {
    HEARTBEAT_COUNT.store(0, Ordering::SeqCst);
}

/// Update the last trigger time (heartbeat)
pub fn update_last_trigger_time() {
    HEARTBEAT_COUNT.fetch_add(1, Ordering::SeqCst);
    *LAST_HOTKEY_TRIGGER_TIME.lock().unwrap() = Some(Instant::now());
}

/// Check if the hotkey was triggered recently (within ms)
pub fn was_triggered_recently(ms: u128) -> bool {
    if let Some(last) = *LAST_HOTKEY_TRIGGER_TIME.lock().unwrap() {
        let elapsed = last.elapsed().as_millis();
        let count = HEARTBEAT_COUNT.load(Ordering::SeqCst);
        let recent = elapsed <= ms;
        // A "Hold" must have been triggered at least twice (initial + at least one repeat)
        let is_hold = recent && count > 1;

        is_hold
    } else {
        false
    }
}

/// Store the hotkey that triggered the current action
pub fn set_current_hotkey(modifiers: u32, vk_code: u32) {
    *CURRENT_HOTKEY.lock().unwrap() = Some((modifiers, vk_code));
}

/// Get the current hotkey info (modifiers, vk_code)
pub fn get_current_hotkey_info() -> Option<(u32, u32)> {
    *CURRENT_HOTKEY.lock().unwrap()
}

/// Check if the current hotkey's modifiers are still being held
/// This uses GetAsyncKeyState to check real-time key state
pub fn are_modifiers_still_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let hotkey = CURRENT_HOTKEY.lock().unwrap().clone();
    if let Some((modifiers, _vk_code)) = hotkey {
        unsafe {
            // Check each modifier
            let alt_required = (modifiers & 0x0001) != 0; // MOD_ALT
            let ctrl_required = (modifiers & 0x0002) != 0; // MOD_CONTROL
            let shift_required = (modifiers & 0x0004) != 0; // MOD_SHIFT
            let win_required = (modifiers & 0x0008) != 0; // MOD_WIN

            let alt_held = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
            let ctrl_held = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            let shift_held = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let lwin_held = (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0;
            let rwin_held = (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0;
            let win_held = lwin_held || rwin_held;

            // RELAXED CHECK: If the user is holding AT LEAST ONE of the required modifiers, we consider it a "Hold".
            // If NO modifiers are required, we check the main key itself.

            let mut satisfied = false;
            let mut debug_str = String::new();

            if modifiers == 0 {
                // Single key hotkey (e.g. F9, `, etc.)
                // Check the key code itself
                // vk_code is usually u32, GetAsyncKeyState expects i32
                let key_held = (GetAsyncKeyState(_vk_code as i32) as u16 & 0x8000) != 0;
                if key_held {
                    satisfied = true;
                }
                debug_str.push_str(&format!("Key({}):{}, ", _vk_code, key_held));
            } else {
                // Modifier combo
                if alt_required {
                    if alt_held {
                        satisfied = true;
                    }
                    debug_str.push_str(&format!("Alt:{}, ", alt_held));
                }
                if ctrl_required {
                    if ctrl_held {
                        satisfied = true;
                    }
                    debug_str.push_str(&format!("Ctrl:{}, ", ctrl_held));
                }
                if shift_required {
                    if shift_held {
                        satisfied = true;
                    }
                    debug_str.push_str(&format!("Shift:{}, ", shift_held));
                }
                if win_required {
                    if win_held {
                        satisfied = true;
                    }
                    debug_str.push_str(&format!("Win:{}, ", win_held));
                }
            }

            println!(
                "[Continuous] Hold check (mods={}): {} -> Satisfied: {}",
                modifiers, debug_str, satisfied
            );
            satisfied
        }
    } else {
        println!("[Continuous] No current hotkey stored.");
        false
    }
}
