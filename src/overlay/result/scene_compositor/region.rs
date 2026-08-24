use super::child::CARDS;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, HRGN, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
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
                union_external_resize_edges(combined, card);
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

unsafe fn union_external_resize_edges(region: HRGN, card: &super::protocol::SceneCard) {
    unsafe {
        let target = HWND(card.id as *mut std::ffi::c_void);
        let edge = resize_edge_width(GetDpiForWindow(target));
        let rect = &card.rect;
        for edge_rect in external_resize_rects(rect.x, rect.y, rect.width, rect.height, edge) {
            union_rect(
                region,
                edge_rect.left,
                edge_rect.top,
                edge_rect.right - edge_rect.left,
                edge_rect.bottom - edge_rect.top,
            );
        }
    }
}

fn resize_edge_width(dpi: u32) -> i32 {
    6_u32.saturating_mul(dpi.max(96)).div_ceil(96) as i32
}

fn external_resize_rects(x: i32, y: i32, width: i32, height: i32, edge: i32) -> [RECT; 4] {
    let right = x + width.max(0);
    let bottom = y + height.max(0);
    let horizontal_edge = edge.max(0).min(height.max(0));
    let vertical_edge = edge.max(0).min(width.max(0));
    [
        RECT {
            left: x,
            top: y,
            right,
            bottom: y + horizontal_edge,
        },
        RECT {
            left: x,
            top: bottom - horizontal_edge,
            right,
            bottom,
        },
        RECT {
            left: x,
            top: y,
            right: x + vertical_edge,
            bottom,
        },
        RECT {
            left: right - vertical_edge,
            top: y,
            right,
            bottom,
        },
    ]
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
    use super::{
        base_bounds, compositor_owns_card_region, external_resize_rects, needs_update,
        resize_edge_width,
    };

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
        assert_eq!(resize_edge_width(96), 6);
        assert_eq!(resize_edge_width(144), 9);

        let edges = external_resize_rects(100, 200, 640, 480, resize_edge_width(144));
        let right = edges[3];
        assert_eq!((right.left, right.right), (731, 740));
        assert_eq!((right.top, right.bottom), (200, 680));
    }
}
