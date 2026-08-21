use crate::overlay::utils::get_clipboard_image_bytes;
use eframe::egui;

/// Get text from Windows clipboard.
fn get_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    unsafe {
        for _attempt in 0..5 {
            if OpenClipboard(None).is_ok() {
                // CF_UNICODETEXT = 13
                if let Ok(h_data) = GetClipboardData(13) {
                    let ptr = GlobalLock(HGLOBAL(h_data.0));
                    if !ptr.is_null() {
                        let wide_ptr = ptr as *const u16;
                        let mut len = 0;
                        while *wide_ptr.add(len) != 0 {
                            len += 1;
                        }
                        let slice = std::slice::from_raw_parts(wide_ptr, len);
                        let text = String::from_utf16_lossy(slice);

                        let _ = GlobalUnlock(HGLOBAL(h_data.0));
                        let _ = CloseClipboard();

                        if !text.is_empty() {
                            return Some(text);
                        }
                        return None;
                    }
                }
                let _ = CloseClipboard();
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

/// The shell only owns paste when an editor does not.
///
/// This is intentionally based on egui's text-edit state instead of widget IDs so every current
/// and future `TextEdit` keeps native paste behavior without joining an exclusion list.
fn shell_owns_paste(ctx: &egui::Context) -> bool {
    !ctx.text_edit_focused()
}

/// Handle Ctrl+V paste - uses Windows API for keyboard detection.
pub fn handle_paste(ctx: &egui::Context) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};

    if crate::gui::settings_ui::help_assistant::is_modal_open() {
        return false;
    }

    let has_focus = ctx.input(|i| i.focused);
    if !has_focus {
        return false;
    }

    if !shell_owns_paste(ctx) {
        return false;
    }

    static LAST_V_STATE: AtomicBool = AtomicBool::new(false);

    let ctrl_down = unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 };
    let v_down = unsafe { (GetAsyncKeyState(VK_V.0 as i32) as u16 & 0x8000) != 0 };
    let v_was_down = LAST_V_STATE.swap(v_down, Ordering::SeqCst);

    let ctrl_v_just_pressed = ctrl_down && v_down && !v_was_down;

    let paste_event = ctx.input(|i| {
        i.raw
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Paste(_)))
    });

    if !ctrl_v_just_pressed && !paste_event {
        return false;
    }

    if let Some(img_bytes) = get_clipboard_image_bytes()
        && let Ok(img) = image::load_from_memory(&img_bytes)
    {
        let rgba = img.to_rgba8();
        std::thread::spawn(move || {
            super::process_image_content(rgba);
        });
        return true;
    }

    if let Some(text) = get_clipboard_text()
        && !text.is_empty()
    {
        std::thread::spawn(move || {
            super::process_text_content(text);
        });
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_text_edit_keeps_paste_ownership() {
        let ctx = egui::Context::default();
        let mut value = String::new();

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::TextEdit::singleline(&mut value))
                .request_focus();
        });

        assert!(ctx.text_edit_focused());
        assert!(!shell_owns_paste(&ctx));
    }

    #[test]
    fn shell_keeps_paste_ownership_without_a_text_editor() {
        let ctx = egui::Context::default();

        assert!(shell_owns_paste(&ctx));
    }
}
