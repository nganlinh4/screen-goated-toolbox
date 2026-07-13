mod clipboard;
mod error_messages;
mod input;

pub use clipboard::{copy_image_to_clipboard, copy_to_clipboard, get_clipboard_image_bytes};
pub use error_messages::{
    get_error_message, should_advance_retry_chain, should_block_retry_provider,
    show_api_key_error_notification,
};
pub use input::{force_focus_and_paste, get_target_window_for_paste, type_text_to_window};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::*;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Shared overlay accent colors used by the realtime/translation WebView overlays.
/// Orange glow for translation windows.
pub const GLOW_TRANSLATION: &str = "#ff9633";
/// Cyan glow for transcription windows.
pub const GLOW_TRANSCRIPTION: &str = "#00c8ff";
/// MD3 primary accent (purple) as raw RGB components, used across egui + WebView surfaces.
pub const ACCENT_PRIMARY_RGB: (u8, u8, u8) = (93u8, 95u8, 239u8);

/// Returns the glow accent color for a realtime overlay window.
/// Translation windows glow orange; transcription windows glow cyan.
pub fn glow_color(is_translation: bool) -> &'static str {
    if is_translation {
        GLOW_TRANSLATION
    } else {
        GLOW_TRANSCRIPTION
    }
}

/// Escape a string for safe insertion as HTML text content / attribute values.
///
/// Escapes `&`, `<`, `>`, `"`, and `'`. Escaping the single quote is a safe
/// superset that keeps the output valid whether it lands inside a double- or
/// single-quoted attribute.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape a string for safe insertion inside a single-quoted JavaScript string
/// literal (e.g. `'...'`).
///
/// Escapes the backslash, single quote, newline (as `\n`) and strips carriage
/// returns. Does not escape double quotes (they are safe inside `'...'`).
pub fn escape_js_single_quoted(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Escape a string for safe insertion inside a double-quoted JavaScript string
/// literal (e.g. `"..."`).
///
/// Escapes the backslash, double quote, newline (as `\n`) and strips carriage
/// returns. Does not escape single quotes (they are safe inside `"..."`).
pub fn escape_js_double_quoted(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Initiate a native window drag from a WebView/IPC handler.
///
/// Releases any active mouse capture, then synchronously posts a non-client
/// left-button-down on the caption so Windows takes over the move loop.
pub fn begin_window_drag(hwnd: HWND) {
    unsafe {
        let _ = ReleaseCapture();
        let _ = SendMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCAPTION as usize)),
            Some(LPARAM(0)),
        );
    }
}

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
        let msg = locale.shell.cannot_type_no_caret;
        drop(app);
        crate::overlay::auto_copy_badge::show_error_notification(msg);
    }
}
