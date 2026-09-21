//! Overlay surfaces are never fullscreen applications, regardless of native bounds.
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, SetPropW};
use windows::core::w;

/// Apply before first show (including creation-time WS_VISIBLE). The Shell retains
/// this property across hide/show, resize and Explorer restarts. NOACTIVATE and
/// clipped window regions alone do not opt out of fullscreen detection.
/// https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist2-markfullscreenwindow
pub(crate) fn prepare(hwnd: HWND) -> windows::core::Result<HWND> {
    unsafe {
        // A documented BOOL-valued property, not an owned kernel handle.
        if let Err(error) = SetPropW(
            hwnd,
            w!("NonRudeHWND"),
            Some(HANDLE(std::ptr::without_provenance_mut(1))),
        ) {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
    }
    Ok(hwnd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    #[test]
    fn overlay_classification_survives_visibility_and_size_changes_without_activation() {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("Static"),
                w!(""),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .and_then(prepare)
            .unwrap();
            assert!(!IsWindowVisible(hwnd).as_bool());
            let foreground = GetForegroundWindow();
            for (width, height) in [(1920, 1080), (3840, 2160), (1, 1)] {
                SetWindowPos(
                    hwnd,
                    None,
                    -32000,
                    -32000,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
                .unwrap();
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                assert_eq!(GetPropW(hwnd, w!("NonRudeHWND")).0 as usize, 1);
                let _ = ShowWindow(hwnd, SW_HIDE);
                assert_eq!(GetPropW(hwnd, w!("NonRudeHWND")).0 as usize, 1);
            }
            assert_eq!(GetForegroundWindow(), foreground);
            DestroyWindow(hwnd).unwrap();
        }
    }

    #[test]
    fn invalid_window_cannot_silently_skip_shell_policy() {
        assert!(prepare(HWND::default()).is_err());
    }
}
