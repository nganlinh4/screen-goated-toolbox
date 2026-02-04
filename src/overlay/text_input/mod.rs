// --- TEXT INPUT MODULE ---
// Text input overlay for user prompts with WebView-based editor.

mod messages;
mod state;
mod styles;
mod window;

use crate::gui::locale::LocaleText;
use state::*;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export state items needed externally
pub use state::{CFG_CANCEL, CFG_LANG, CFG_TITLE};

// --- PUBLIC API ---

pub fn is_active() -> bool {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return false;
    }
    unsafe {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        // Since we use offscreen WS_VISIBLE, we must check coordinates
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            return rect.left > -3000;
        }
        false
    }
}

pub fn cancel_input() {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Set text content in the webview editor (for paste operations)
/// This is thread-safe and can be called from any thread
pub fn set_editor_text(text: &str) {
    // Store the text in the mutex
    *PENDING_TEXT.lock().unwrap() = Some(text.to_string());

    // Post message to the text input window to trigger the injection
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_SET_TEXT,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

/// Clear the webview editor content and refocus (for continuous input mode)
pub fn clear_editor_text() {
    messages::clear_editor_text();
}

/// Update the UI text (header) and trigger a repaint
pub fn update_ui_text(header_text: String) {
    *CFG_TITLE.lock().unwrap() = header_text.clone();
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            let _ = SetWindowTextW(hwnd, &windows::core::HSTRING::from(header_text));
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(1), LPARAM(0));
        }
    }
}

/// Bring the text input window to foreground and focus the editor
/// Call this after closing modal windows like the preset wheel
pub fn refocus_editor() {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, SetForegroundWindow, SetTimer,
            };

            // Aggressive focus: try multiple methods
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));

            // Focus the webview editor immediately
            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    // First focus the WebView itself (native focus)
                    let _ = wv.focus();
                    // Then focus the textarea inside via JavaScript
                    let _ = wv.evaluate_script("document.getElementById('editor').focus();");
                }
            });

            // Schedule another focus attempt after 200ms via timer ID 3
            // This will be handled in WM_TIMER in the same thread
            let _ = SetTimer(Some(hwnd), 3, 200, None);
        }
    }
}

/// Get the current window rect of the text input window (if active)
pub fn get_window_rect() -> Option<RECT> {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(HWND(hwnd_val as *mut std::ffi::c_void), &mut rect).is_ok() {
                return Some(rect);
            }
        }
    }
    None
}

/// Start the persistent hidden window (called from main)
pub fn warmup() {
    // Thread-safe atomic check-and-set to prevent multiple warmup threads
    if IS_WARMED_UP.load(Ordering::SeqCst) {
        return;
    }
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        window::internal_create_window_loop();
    });
}

pub fn show(
    prompt_guide: String,
    ui_language: String,
    cancel_hotkey_name: String,
    continuous_mode: bool,
    on_submit: impl Fn(String, HWND) + Send + 'static,
) {
    // Re-entrancy guard: if we are already in the process of showing/waiting, ignore subsequent calls
    // This prevents key-mashing from spawning multiple wait loops or confused states
    if IS_SHOWING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    // Ensure we clear the flag when we return
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            IS_SHOWING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    // Clone lang for locale notification before moving/consuming it
    let lang_for_locale = ui_language.clone();

    // Update shared state FIRST so it's ready when window shows up
    *CFG_TITLE.lock().unwrap() = prompt_guide;
    *CFG_LANG.lock().unwrap() = ui_language;
    *CFG_CANCEL.lock().unwrap() = cancel_hotkey_name;
    *CFG_CONTINUOUS.lock().unwrap() = continuous_mode;
    *CFG_CALLBACK.lock().unwrap() = Some(Box::new(on_submit));

    *SUBMITTED_TEXT.lock().unwrap() = None;
    *SHOULD_CLOSE.lock().unwrap() = false;
    *SHOULD_CLEAR_ONLY.lock().unwrap() = false;

    // Check if warmed up
    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        // Trigger warmup for recovery
        warmup();

        // Show localized message that feature is not ready yet
        let locale = LocaleText::get(&lang_for_locale);
        crate::overlay::auto_copy_badge::show_notification(locale.text_input_loading);

        // Blocking wait with message pump
        // We wait up to 20 seconds. If it fails, we simply return (preventing premature broken window)
        for _ in 0..2000 {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(10));

            if IS_WARMED_UP.load(Ordering::SeqCst) {
                break;
            }
        }

        // If still not warmed up after wait, give up
        if !IS_WARMED_UP.load(Ordering::SeqCst) {
            return;
        }
    }

    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            // ALWAYS show logic (Toggle logic handled by caller if needed)
            // Fixes issue where dynamic prompt mode fails to appear if window state is desync
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}
