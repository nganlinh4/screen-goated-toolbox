//! Clipboard get/set for the Computer Control agent. Used to PASTE text fast
//! (synthesizing a keystroke per character is painfully slow for long text and
//! mangles non-ASCII like Vietnamese), and as a general low-level control the
//! model can read directly.

use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

/// CF_UNICODETEXT clipboard format.
const CF_UNICODETEXT: u32 = 13;
/// Non-text formats we must not clobber: CF_BITMAP, CF_DIB, CF_HDROP (files).
const NONTEXT_FORMATS: [u32; 3] = [2, 8, 15];

/// The current clipboard text (empty if none / not text).
pub(super) fn get_text() -> String {
    unsafe {
        let mut out = String::new();
        if OpenClipboard(Some(HWND::default())).is_ok() {
            if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
                let hg = HGLOBAL(h.0);
                let ptr = GlobalLock(hg) as *const u16;
                if !ptr.is_null() {
                    let size = GlobalSize(hg);
                    let slice = std::slice::from_raw_parts(ptr, size / 2);
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                    out = String::from_utf16_lossy(&slice[..end]);
                }
                let _ = GlobalUnlock(hg);
            }
            let _ = CloseClipboard();
        }
        out
    }
}

/// Set the clipboard text (reuses the app's proven, retrying setter).
pub(super) fn set_text(text: &str) {
    crate::overlay::utils::copy_to_clipboard(text, HWND::default());
}

/// Empty the clipboard - so a paste-typing fast-path doesn't leave OUR text on a
/// clipboard the user had empty.
pub(super) fn clear() {
    unsafe {
        if OpenClipboard(Some(HWND::default())).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }
}

/// True if the clipboard holds NON-text content (an image or copied files) that a
/// paste fast-path would destroy - the caller should type by keystroke instead.
pub(super) fn has_nontext() -> bool {
    NONTEXT_FORMATS
        .iter()
        .any(|&f| unsafe { IsClipboardFormatAvailable(f).is_ok() })
}
