//! Windows announces a light/dark switch with a WM_SETTINGCHANGE broadcast for
//! the "ImmersiveColorSet" area. The hotkey listener window receives it even
//! while the settings window is hidden, and records it here so the next frame
//! re-reads the theme at once instead of waiting for the next poll.

use std::sync::atomic::{AtomicBool, Ordering};

static THEME_CHANGED: AtomicBool = AtomicBool::new(false);

const COLOR_SET_AREA: &str = "ImmersiveColorSet";

/// `area` is the NUL-terminated wide string WM_SETTINGCHANGE carries in lParam,
/// or null for a broadcast without one.
///
/// # Safety
/// A non-null `area` must point to a readable NUL-terminated UTF-16 string.
pub(crate) unsafe fn note_setting_change(area: *const u16) {
    if area.is_null() {
        return;
    }
    let area = windows::core::PCWSTR(area);
    if record_setting_change(&THEME_CHANGED, unsafe { area.as_wide() }) {
        request_repaint();
    }
}

/// True once per announced theme change.
pub(crate) fn take_theme_change() -> bool {
    THEME_CHANGED.swap(false, Ordering::AcqRel)
}

fn record_setting_change(flag: &AtomicBool, area: &[u16]) -> bool {
    let theme = area.iter().copied().eq(COLOR_SET_AREA.encode_utf16());
    if theme {
        flag.store(true, Ordering::Release);
    }
    theme
}

fn request_repaint() {
    let context = crate::gui::GUI_CONTEXT
        .lock()
        .ok()
        .and_then(|context| context.clone());
    if let Some(context) = context {
        context.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn only_the_colour_set_broadcast_marks_a_theme_change() {
        let flag = AtomicBool::new(false);
        assert!(!record_setting_change(&flag, &wide("Environment")));
        assert!(!record_setting_change(&flag, &wide("ImmersiveColorSetX")));
        assert!(!flag.load(Ordering::Acquire));
        assert!(record_setting_change(&flag, &wide("ImmersiveColorSet")));
        assert!(flag.load(Ordering::Acquire));
    }

    #[test]
    fn the_listener_message_reaches_the_next_theme_poll_once() {
        let area: Vec<u16> = wide("ImmersiveColorSet")
            .into_iter()
            .chain(std::iter::once(0))
            .collect();
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE;
        let deliver = |lparam: isize| unsafe {
            crate::hotkey::hotkey_proc(HWND::default(), WM_SETTINGCHANGE, WPARAM(0), LPARAM(lparam))
        };
        deliver(0);
        assert!(!take_theme_change());
        deliver(area.as_ptr() as isize);
        assert!(take_theme_change());
        assert!(!take_theme_change());
    }
}
