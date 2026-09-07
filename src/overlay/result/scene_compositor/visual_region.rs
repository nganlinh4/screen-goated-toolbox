//! Native visibility and clipping for the render-only host.
use super::protocol::{SceneCard, SceneRect};
use std::collections::HashMap;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EqualRgn, GetWindowRgn, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    ShowWindow,
};

pub(super) fn hide(hwnd: HWND) {
    unsafe {
        // Visibility is the fail-closed boundary even if allocating a region fails.
        let _ = ShowWindow(hwnd, SW_HIDE);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let _ = SetWindowPos(
            hwnd,
            None,
            GetSystemMetrics(SM_XVIRTUALSCREEN)
                .saturating_sub(width)
                .saturating_sub(64),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            width,
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        if !super::child::set_renderer_visible(hwnd, false) {
            std::process::exit(1);
        }
        let empty = CreateRectRgn(0, 0, 0, 0);
        if !empty.is_invalid() && SetWindowRgn(hwnd, Some(empty), false) == 0 {
            let _ = DeleteObject(empty.into());
        }
    }
}

pub(super) fn update(
    hwnd: HWND,
    cards: &HashMap<isize, SceneCard>,
    buttons: &[SceneRect],
    width: i32,
    height: i32,
) -> bool {
    let mut rects = Vec::new();
    for card in cards.values().filter(|card| card.visible) {
        rects.push(card.rect.clone());
        rects.push(card.control_rect.clone());
    }
    if rects.is_empty() {
        hide(hwnd);
        return true;
    }
    rects.extend_from_slice(buttons);
    rects.extend(super::gesture::visual_preview_rects(cards));
    apply(hwnd, &rects, width, height)
}

fn apply(hwnd: HWND, rects: &[SceneRect], width: i32, height: i32) -> bool {
    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        if combined.is_invalid() {
            hide(hwnd);
            return false;
        }
        let mut count = 0;
        for rect in rects {
            let left = rect.x.clamp(0, width);
            let top = rect.y.clamp(0, height);
            let right = rect.x.saturating_add(rect.width.max(0)).clamp(0, width);
            let bottom = rect.y.saturating_add(rect.height.max(0)).clamp(0, height);
            if right <= left || bottom <= top {
                continue;
            }
            let part = CreateRectRgn(left, top, right, bottom);
            let valid = !part.is_invalid()
                && CombineRgn(Some(combined), Some(combined), Some(part), RGN_OR).0 != 0;
            if !part.is_invalid() {
                let _ = DeleteObject(part.into());
            }
            if !valid {
                let _ = DeleteObject(combined.into());
                hide(hwnd);
                return false;
            }
            count += 1;
        }
        let current = CreateRectRgn(0, 0, 0, 0);
        let unchanged = !current.is_invalid()
            && GetWindowRgn(hwnd, current).0 != 0
            && EqualRgn(current, combined).as_bool();
        if !current.is_invalid() {
            let _ = DeleteObject(current.into());
        }
        if unchanged {
            let _ = DeleteObject(combined.into());
        } else if SetWindowRgn(hwnd, Some(combined), true) == 0 {
            let _ = DeleteObject(combined.into());
            hide(hwnd);
            return false;
        }
        let visible = IsWindowVisible(hwnd).as_bool();
        if count == 0 {
            hide(hwnd);
        } else if count > 0 && !visible {
            if SetWindowPos(
                hwnd,
                None,
                super::compositor_host_x(GetSystemMetrics(SM_XVIRTUALSCREEN), width),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .is_err()
                || !super::child::set_renderer_visible(hwnd, true)
            {
                hide(hwnd);
                return false;
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::PtInRegion;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

    #[test]
    fn visual_host_starts_empty_and_returns_to_empty_after_content() {
        let hwnd = super::super::child::create_host_window().unwrap();
        unsafe {
            assert!(!IsWindowVisible(hwnd).as_bool());
            let mut bounds = windows::Win32::Foundation::RECT::default();
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
            assert!(bounds.right < GetSystemMetrics(SM_XVIRTUALSCREEN));
            let region = CreateRectRgn(0, 0, 0, 0);
            assert_eq!(GetWindowRgn(hwnd, region).0, 1);
            let rect = SceneRect {
                x: 100,
                y: 100,
                width: 120,
                height: 80,
            };
            assert!(apply(hwnd, &[rect], 800, 600));
            assert!(IsWindowVisible(hwnd).as_bool());
            assert_eq!(GetWindowRgn(hwnd, region).0, 2);
            assert!(PtInRegion(region, 150, 150).as_bool());
            assert!(!PtInRegion(region, 400, 300).as_bool());
            assert!(apply(hwnd, &[], 800, 600));
            assert!(!IsWindowVisible(hwnd).as_bool());
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
            assert!(bounds.right < GetSystemMetrics(SM_XVIRTUALSCREEN));
            assert_eq!(GetWindowRgn(hwnd, region).0, 1);
            let _ = DeleteObject(region.into());
            let _ = DestroyWindow(hwnd);
        }
    }
}
