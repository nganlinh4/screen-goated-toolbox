use windows::Win32::Foundation::{HWND, RECT};

pub(super) fn update(hwnd: HWND, _redraw: bool) {
    let target_hwnd = {
        let input_val = super::child::INPUT_SURFACE_HWND.load(std::sync::atomic::Ordering::SeqCst);
        if input_val != 0 {
            HWND(input_val as *mut std::ffi::c_void)
        } else {
            hwnd
        }
    };
    let cards = super::child::CARDS.lock().unwrap();
    let button_regions = super::button_input::interactive_regions();
    let width = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CXVIRTUALSCREEN,
        )
        .max(1)
    };
    let height = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CYVIRTUALSCREEN,
        )
        .max(1)
    };
    let x = super::compositor_host_x(
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
            )
        },
        width,
    );
    let y = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
        )
    };
    if !super::visual_region::update(hwnd, &cards, &button_regions, width, height) {
        super::visual_region::hide(target_hwnd);
        std::process::exit(1);
    }
    let applied = super::input_surface::update_input_regions(
        target_hwnd,
        &cards,
        &button_regions,
        x,
        y,
        width,
        height,
    );
    if applied.is_hidden {
        super::visual_region::hide(hwnd);
    }
}

pub(super) fn resize_edge_width(dpi: u32) -> i32 {
    6_u32.saturating_mul(dpi.max(96)).div_ceil(96) as i32
}

pub(super) fn external_resize_rects(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    edge: i32,
) -> [RECT; 4] {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn compositor_owns_card_region(external_navigation: bool) -> bool {
        !external_navigation
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

    #[test]
    fn resize_edge_bounds_stay_within_card_geometry() {
        let edges = external_resize_rects(0, 0, 100, 100, 10);
        assert_eq!(edges[0].top, 0);
        assert_eq!(edges[0].bottom, 10);
        assert_eq!(edges[1].top, 90);
        assert_eq!(edges[1].bottom, 100);
        assert_eq!(edges[2].left, 0);
        assert_eq!(edges[2].right, 10);
        assert_eq!(edges[3].left, 90);
        assert_eq!(edges[3].right, 100);
    }
}
