use crate::overlay::utils::get_clipboard_image_bytes;
use eframe::egui;

const MAX_CLIPBOARD_TEXT_BYTES: usize = super::file_load::MAX_TEXT_BYTES;

/// Decode only complete, bounded, NUL-terminated CF_UNICODETEXT data.
fn decode_clipboard_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return None;
    }
    let mut units = Vec::new();
    for pair in bytes.as_chunks::<2>().0 {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            return Some(String::from_utf16_lossy(&units));
        }
        units.push(unit);
    }
    None
}

fn decode_pasted_image(bytes: &[u8]) -> anyhow::Result<image::RgbaImage> {
    Ok(crate::image_decode::load_from_memory(bytes)?.to_rgba8())
}

/// Get text from Windows clipboard.
fn get_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    struct GlobalLockGuard(HGLOBAL);
    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = GlobalUnlock(self.0);
            }
        }
    }

    unsafe {
        for _attempt in 0..5 {
            if OpenClipboard(None).is_ok() {
                let _clipboard = ClipboardGuard;
                let handle = HGLOBAL(GetClipboardData(13).ok()?.0);
                let size = GlobalSize(handle);
                if size == 0 || size > MAX_CLIPBOARD_TEXT_BYTES {
                    return None;
                }
                let ptr = GlobalLock(handle).cast::<u8>();
                if ptr.is_null() {
                    return None;
                }
                let _lock = GlobalLockGuard(handle);
                let bytes = std::slice::from_raw_parts(ptr, size);
                return decode_clipboard_text(bytes).filter(|text| !text.is_empty());
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

    if let Some(img_bytes) = get_clipboard_image_bytes() {
        // Decoding stays off the UI thread and goes through the shared bounded
        // decoder; an undecodable image still falls back to clipboard text.
        std::thread::spawn(move || match decode_pasted_image(&img_bytes) {
            Ok(rgba) => super::process_image_content(rgba),
            Err(error) => {
                crate::log_info!("[paste] clipboard image rejected: {error:#}");
                if let Some(text) = get_clipboard_text() {
                    super::process_text_content(text);
                }
            }
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
    fn pasted_images_use_the_shared_bounded_decoder() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(decode_pasted_image(&png).unwrap().dimensions(), (3, 2));

        let oversized = vec![0_u8; crate::image_decode::MAX_ENCODED_BYTES + 1];
        assert!(decode_pasted_image(&oversized).is_err());
    }

    #[test]
    fn every_native_text_edit_keeps_paste_ownership() {
        let ctx = egui::Context::default();
        let mut value = String::new();

        let _ = crate::gui::test_support::run_ui(&ctx, egui::RawInput::default(), |ui| {
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

    #[test]
    fn bounded_clipboard_text_requires_a_complete_terminator() {
        let text = "Sample 한글";
        let bytes: Vec<u8> = text
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode_clipboard_text(&bytes).as_deref(), Some(text));
        assert_eq!(
            decode_clipboard_text(&[b'A', 0, 0, 0, b'B', 0]).as_deref(),
            Some("A")
        );
        assert_eq!(
            decode_clipboard_text(&[b'A', 0, 0, 0, 0xff]).as_deref(),
            Some("A")
        );
        assert_eq!(decode_clipboard_text(&[b'A', 0]), None);
        assert_eq!(decode_clipboard_text(&[b'A', 0, 0]), None);
        assert_eq!(decode_clipboard_text(&[]), None);
        assert_eq!(decode_clipboard_text(&[0, 0]).as_deref(), Some(""));
    }

    #[test]
    fn bounded_clipboard_text_handles_invalid_utf16_and_size_limit() {
        assert_eq!(
            decode_clipboard_text(&[0x00, 0xd8, 0, 0]).as_deref(),
            Some("�")
        );
        let mut at_limit = vec![b'A', 0];
        at_limit.resize(MAX_CLIPBOARD_TEXT_BYTES, 0);
        assert_eq!(decode_clipboard_text(&at_limit).as_deref(), Some("A"));
        at_limit.extend([0, 0]);
        assert_eq!(decode_clipboard_text(&at_limit), None);

        let mut full_scan = [b'A', 0].repeat(MAX_CLIPBOARD_TEXT_BYTES / 2);
        let end = full_scan.len();
        full_scan[end - 2] = 0;
        assert_eq!(
            decode_clipboard_text(&full_scan).map(|text| text.len()),
            Some(MAX_CLIPBOARD_TEXT_BYTES / 2 - 1)
        );
        full_scan[end - 2] = b'A';
        assert_eq!(decode_clipboard_text(&full_scan), None);
    }
}
