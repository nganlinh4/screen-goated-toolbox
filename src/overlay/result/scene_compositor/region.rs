use super::child::CARDS;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, HRGN, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HWND_TOPMOST, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
};

pub(super) fn needs_update(redraw: bool, dragging: bool) -> bool {
    redraw || !dragging
}

fn compositor_owns_card_region(external_navigation: bool) -> bool {
    !external_navigation
}

fn base_bounds(dragging: bool, width: i32, height: i32) -> RECT {
    if dragging {
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    } else {
        RECT::default()
    }
}

pub(super) fn update(hwnd: HWND, redraw: bool) {
    unsafe {
        let dragging = super::button_input::is_dragging();
        let bounds = base_bounds(
            dragging,
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        );
        let combined = CreateRectRgn(bounds.left, bounds.top, bounds.right, bounds.bottom);
        let cards = CARDS.lock().unwrap();
        let mut visible_count = 0usize;
        for card in cards.values().filter(|card| card.visible) {
            visible_count += 1;
            if !compositor_owns_card_region(card.external_navigation) {
                continue;
            }
            union_rect(
                combined,
                card.rect.x,
                card.rect.y,
                card.rect.width,
                card.rect.height,
            );
        }
        if !dragging {
            for region in super::button_input::interactive_regions() {
                union_rect(combined, region.x, region.y, region.width, region.height);
            }
        }
        drop(cards);
        let _ = SetWindowRgn(hwnd, Some(combined), redraw);
        if visible_count == 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

unsafe fn union_rect(region: HRGN, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let rect = CreateRectRgn(x, y, x + width, y + height);
        let _ = CombineRgn(Some(region), Some(region), Some(rect), RGN_OR);
        let _ = DeleteObject(rect.into());
    }
}

#[cfg(test)]
mod tests {
    use super::{base_bounds, compositor_owns_card_region, needs_update};

    #[test]
    fn drag_uses_the_full_compositor_without_moving_the_native_clip() {
        let bounds = base_bounds(true, 2560, 1080);
        assert_eq!((bounds.left, bounds.top), (0, 0));
        assert_eq!((bounds.right, bounds.bottom), (2560, 1080));
        assert!(!needs_update(false, true));
        assert!(needs_update(false, false));
        assert!(needs_update(true, true));
    }

    #[test]
    fn external_navigation_leaves_a_hole_in_the_shared_compositor() {
        assert!(!compositor_owns_card_region(true));
        assert!(compositor_owns_card_region(false));
    }
}
