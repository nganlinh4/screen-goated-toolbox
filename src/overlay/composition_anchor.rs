//! Keep the composition controller's native input plane outside the desktop.
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WS_CHILD, WS_EX_NOACTIVATE,
    WS_VISIBLE,
};
use windows::core::w;

// The visual target and physical input surface remain on screen. The controller
// receives their explicitly forwarded input and renders into the visual target;
// its own native bounds must not introduce another desktop input surface.
pub(crate) fn create(parent: HWND, width: i32, height: i32) -> windows::core::Result<HWND> {
    unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("Static"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            offset(width),
            0,
            width.max(1),
            height.max(1),
            Some(parent),
            None,
            None,
            None,
        )
    }
}

pub(crate) fn resize(hwnd: HWND, width: i32, height: i32) -> windows::core::Result<()> {
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            offset(width),
            0,
            width.max(1),
            height.max(1),
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
}

fn offset(width: i32) -> i32 {
    width.max(1).saturating_add(64).saturating_neg()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, GetSystemMetrics, GetWindowRect, IsChild, IsWindow, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WS_POPUP,
    };

    #[test]
    fn controller_plane_stays_outside_the_desktop_across_viewport_resizes() {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
            let parent = CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("Static"),
                w!(""),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let anchor = create(parent, width, height).unwrap();
            assert!(IsChild(parent, anchor).as_bool());
            for (width, height) in [(width, height), (1, 1), (width + 1920, height + 1080)] {
                resize(anchor, width, height).unwrap();
                let mut bounds = RECT::default();
                GetWindowRect(anchor, &mut bounds).unwrap();
                assert!(bounds.right < x);
                assert_eq!(bounds.right - bounds.left, width);
                assert_eq!(bounds.bottom - bounds.top, height);
            }
            DestroyWindow(parent).unwrap();
            assert!(!IsWindow(Some(anchor)).as_bool());
        }
    }
}
