mod clipboard;
mod error_messages;
mod input;

pub use clipboard::{copy_image_to_clipboard, copy_to_clipboard, get_clipboard_image_bytes};
pub use error_messages::{
    get_error_message, should_advance_retry_chain, should_block_retry_provider,
};
pub use input::{force_focus_and_paste, get_target_window_for_paste, type_text_to_window};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Timestamp (millis since epoch) of last "no caret" error badge.
/// Used to rate-limit error notifications during streaming typing.
pub(crate) static LAST_NO_CARET_ERROR_MS: AtomicU64 = AtomicU64::new(0);
pub(crate) const NO_CARET_ERROR_COOLDOWN_MS: u64 = 5000; // Show error at most once per 5 seconds

/// Checks if there's a text input element focused using UI Automation.
/// This works for modern apps (Chrome, VS Code, Electron) unlike the legacy caret API.
/// Returns true if a text input is focused, false otherwise.
pub fn is_text_input_focused() -> bool {
    unsafe {
        // Try UI Automation first (works for modern apps)
        // We use a pattern-based approach which is robust for Chrome/Electron/VSCode
        if let Ok(uia) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            && let Ok(focused) = uia.GetFocusedElement()
        {
            // Check for ValuePattern (simpler text inputs)
            // UIA_ValuePatternId = 10002
            if focused.GetCurrentPattern(UIA_ValuePatternId).is_ok() {
                return true;
            }

            // Check for TextPattern (rich text editors)
            // UIA_TextPatternId = 10014
            if focused.GetCurrentPattern(UIA_TextPatternId).is_ok() {
                return true;
            }
        }

        // Fallback: Check legacy Win32 caret (for traditional Win32 apps like Notepad)
        let hwnd_foreground = GetForegroundWindow();
        if !hwnd_foreground.is_invalid() {
            let thread_id = GetWindowThreadProcessId(hwnd_foreground, None);
            if thread_id != 0 {
                let mut gui_info = GUITHREADINFO {
                    cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                    ..Default::default()
                };

                if GetGUIThreadInfo(thread_id, &mut gui_info).is_ok() {
                    let has_caret = !gui_info.hwndCaret.is_invalid();
                    let blinking = (gui_info.flags & GUI_CARETBLINKING).0 != 0;

                    if has_caret || blinking {
                        return true;
                    }
                }
            }
        }

        false
    }
}

pub fn to_wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Global switch for the "context quote" displayed in result windows during refining.
/// Set to false to hide the quote and only show the glow animation.
pub const SHOW_REFINING_CONTEXT_QUOTE: bool = false;

pub fn get_context_quote(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let len = words.len();
    if len > 50 {
        format!("\"... {}\"", words[len - 50..].join(" "))
    } else {
        format!("\"... {}\"", words.join(" "))
    }
}

/// Rate-limited warning when no writable text input is detected.
/// Shows an error notification at most once per `NO_CARET_ERROR_COOLDOWN_MS`.
pub(super) fn warn_no_caret() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last_error_ms = LAST_NO_CARET_ERROR_MS.load(Ordering::Relaxed);

    if now_ms.saturating_sub(last_error_ms) >= NO_CARET_ERROR_COOLDOWN_MS {
        LAST_NO_CARET_ERROR_MS.store(now_ms, Ordering::Relaxed);
        let app = crate::APP.lock().unwrap();
        let ui_lang = app.config.ui_language.clone();
        let locale = crate::gui::locale::LocaleText::get(&ui_lang);
        let msg = locale.cannot_type_no_caret;
        drop(app);
        crate::overlay::auto_copy_badge::show_error_notification(msg);
    }
}
