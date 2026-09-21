//! Native visibility and clipping for the render-only host.
use super::protocol::{SceneCard, SceneRect};
use std::collections::HashMap;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EqualRgn, GetWindowRgn, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GW_HWNDNEXT, GetSystemMetrics, GetWindow, IsWindowVisible, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetWindowPos, ShowWindow,
};

pub(super) fn stack_below_input(visual: HWND, input: HWND) -> bool {
    unsafe {
        if visual == input
            || !IsWindowVisible(visual).as_bool()
            || !IsWindowVisible(input).as_bool()
            || GetWindow(input, GW_HWNDNEXT).ok() == Some(visual)
        {
            return true;
        }
        // Raising input alone can leave another topmost app between input and
        // its pixels. Keep the pair adjacent without activation or geometry changes.
        let positioned = SetWindowPos(
            visual,
            Some(input),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .is_ok();
        if positioned {
            super::child::request_stack_reconciliation();
        }
        positioned
    }
}

pub(super) fn hide(hwnd: HWND) {
    unsafe {
        // Visibility is the fail-closed boundary even if allocating a region fails.
        if IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        // Native visibility and the off-screen controller anchor already exclude
        // input. A failed optional render-throttling request must not kill the child.
        let _ = super::child::set_renderer_visible(hwnd, false);
        let mut bounds = windows::Win32::Foundation::RECT::default();
        if windows::Win32::Graphics::Gdi::GetWindowRgnBox(hwnd, &mut bounds)
            == windows::Win32::Graphics::Gdi::NULLREGION
        {
            return;
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
    rects.extend(super::processing::visual_rects());
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
        // DirectComposition owns the pixels. Region changes must not request a
        // GDI erase/repaint of the shared surface and its already-visible cards.
        } else if SetWindowRgn(hwnd, Some(combined), false) == 0 {
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
            assert_eq!(
                windows::Win32::UI::WindowsAndMessaging::GetPropW(
                    hwnd,
                    windows::core::w!("NonRudeHWND")
                )
                .0 as usize,
                1
            );
            assert!(!IsWindowVisible(hwnd).as_bool());
            let mut bounds = windows::Win32::Foundation::RECT::default();
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
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
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
            let shown_bounds = bounds;
            assert!(apply(hwnd, &[], 800, 600));
            assert!(!IsWindowVisible(hwnd).as_bool());
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
            assert_eq!(bounds, shown_bounds);
            assert_eq!(GetWindowRgn(hwnd, region).0, 1);
            for _ in 0..20 {
                hide(hwnd);
            }
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut bounds).unwrap();
            assert_eq!(bounds, shown_bounds);
            assert!(!IsWindowVisible(hwnd).as_bool());
            assert_eq!(GetWindowRgn(hwnd, region).0, 1);
            let _ = DeleteObject(region.into());
            let _ = DestroyWindow(hwnd);
        }
    }
}
