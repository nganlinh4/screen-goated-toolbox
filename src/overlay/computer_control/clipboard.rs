//! Clipboard get/set for explicit Computer Control operations.

use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::core::w;

/// CF_UNICODETEXT clipboard format.
const CF_UNICODETEXT: u32 = 13;
/// Content a text publication would destroy with no text equivalent: bitmaps,
/// metafiles, TIFF, DIB/DIBV5 and copied files (CF_HDROP).
const IRREPLACEABLE_FORMATS: [u32; 7] = [2, 3, 6, 8, 14, 15, 17];
const MAX_QUERY_UNITS: usize = 8 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum QueryError {
    Unavailable,
    TooLarge,
    Unterminated,
}

impl QueryError {
    pub(super) fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "clipboard_unavailable",
            Self::TooLarge => "clipboard_too_large",
            Self::Unterminated => "clipboard_invalid",
        }
    }

    pub(super) fn message(&self) -> &'static str {
        match self {
            Self::Unavailable => "Clipboard text is unavailable.",
            Self::TooLarge => "Clipboard text exceeds the query size limit.",
            Self::Unterminated => "Clipboard text is not NUL-terminated.",
        }
    }
}

fn decode_query(units: &[u16], has_more: bool) -> Result<String, QueryError> {
    if let Some(end) = units.iter().position(|unit| *unit == 0) {
        if end > MAX_QUERY_UNITS {
            return Err(QueryError::TooLarge);
        }
        return Ok(String::from_utf16_lossy(&units[..end]));
    }
    if has_more || units.len() > MAX_QUERY_UNITS {
        Err(QueryError::TooLarge)
    } else {
        Err(QueryError::Unterminated)
    }
}

/// Read model-facing clipboard text with a bounded scan and allocation.
pub(super) fn get_text_for_query() -> Result<String, QueryError> {
    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
    struct GlobalGuard(HGLOBAL);
    impl Drop for GlobalGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = GlobalUnlock(self.0);
            }
        }
    }
    unsafe {
        if OpenClipboard(Some(HWND::default())).is_err() {
            return Err(QueryError::Unavailable);
        }
        let _clipboard = ClipboardGuard;
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return Ok(String::new());
        }
        let handle = HGLOBAL(
            GetClipboardData(CF_UNICODETEXT)
                .map_err(|_| QueryError::Unavailable)?
                .0,
        );
        let size = GlobalSize(handle);
        if size < 2 {
            return Err(QueryError::Unterminated);
        }
        let ptr = GlobalLock(handle).cast::<u16>();
        if ptr.is_null() {
            return Err(QueryError::Unavailable);
        }
        let _locked = GlobalGuard(handle);
        let available_units = size / 2;
        let examined = available_units.min(MAX_QUERY_UNITS + 1);
        let units = std::slice::from_raw_parts(ptr, examined);
        decode_query(units, available_units > examined)
    }
}

/// Publish text and return the clipboard sequence that identifies exactly this
/// publication, so a later paste can prove the clipboard was not replaced.
/// Agent-staged content is kept out of Clipboard History and cloud sync.
pub(super) fn stage_text(text: &str) -> Option<u32> {
    crate::overlay::utils::copy_private_text_with_sequence(text, HWND::default())
}

/// Current system clipboard sequence; any replacement increments it.
pub(super) fn sequence() -> u32 {
    crate::overlay::utils::paste_diagnostics::clipboard_sequence()
}

/// True if replacing the clipboard would destroy an image or copied files.
pub(super) fn holds_irreplaceable_content() -> bool {
    let png = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    any_irreplaceable(png, |format| unsafe {
        IsClipboardFormatAvailable(format).is_ok()
    })
}

fn any_irreplaceable(png_format: u32, available: impl Fn(u32) -> bool) -> bool {
    IRREPLACEABLE_FORMATS
        .iter()
        .copied()
        .chain((png_format != 0).then_some(png_format))
        .any(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_files_and_metafiles_are_irreplaceable_but_text_families_are_not() {
        const PNG: u32 = 0xC0F0;
        for format in [2, 3, 6, 8, 14, 15, 17, PNG] {
            assert!(
                any_irreplaceable(PNG, |present| present == format),
                "{format}"
            );
        }
        // CF_TEXT, CF_OEMTEXT, CF_UNICODETEXT, CF_LOCALE and a registered
        // rich-text format all have a text equivalent.
        for format in [1, 7, 13, 16, 0xC0F1] {
            assert!(
                !any_irreplaceable(PNG, |present| present == format),
                "{format}"
            );
        }
        assert!(!any_irreplaceable(0, |present| present == 0));
    }

    #[test]
    fn query_accepts_terminated_unicode_even_in_a_larger_allocation() {
        let mut units: Vec<u16> = "Hello 𠀀".encode_utf16().collect();
        units.push(0);
        assert_eq!(decode_query(&units, true).unwrap(), "Hello 𠀀");
    }

    #[test]
    fn query_distinguishes_oversize_from_unterminated_and_accepts_boundary() {
        let mut boundary = vec![b'a' as u16; MAX_QUERY_UNITS];
        boundary.push(0);
        assert_eq!(
            decode_query(&boundary, false).unwrap().len(),
            MAX_QUERY_UNITS
        );
        boundary[MAX_QUERY_UNITS] = b'b' as u16;
        assert_eq!(decode_query(&boundary, false), Err(QueryError::TooLarge));
        assert_eq!(
            decode_query(&[65, 66], false),
            Err(QueryError::Unterminated)
        );
    }
}
