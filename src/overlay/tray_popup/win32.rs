use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GA_ROOT, GetAncestor, GetCursorPos, GetForegroundWindow, HWND_TOPMOST,
    SWP_NOACTIVATE, SetForegroundWindow, SetWindowPos,
};
use windows::core::{HSTRING, PCWSTR};

use super::layout::{
    FLYOUT_GAP, FLYOUT_WIDTH, MAIN_HEIGHT, MAIN_WIDTH, PhysicalPoint, PopupPlacement, WorkArea,
};

pub(super) struct MonitorMetrics {
    pub work_area: WorkArea,
    pub pixels_per_point: f32,
}

pub(super) fn cursor_position() -> PhysicalPoint {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    PhysicalPoint {
        x: point.x,
        y: point.y,
    }
}

pub(super) fn monitor_metrics(anchor: PhysicalPoint, zoom_factor: f32) -> MonitorMetrics {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: anchor.x,
                y: anchor.y,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let has_info = GetMonitorInfoW(monitor, &mut info).as_bool();
        let work_area = if has_info {
            WorkArea {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            }
        } else {
            WorkArea {
                left: anchor.x - 960,
                top: anchor.y - 540,
                right: anchor.x + 960,
                bottom: anchor.y + 540,
            }
        };

        let mut dpi_x = 0;
        let mut dpi_y = 0;
        let dpi = if GetDpiForMonitor(
            monitor,
            MDT_EFFECTIVE_DPI,
            std::ptr::addr_of_mut!(dpi_x),
            std::ptr::addr_of_mut!(dpi_y),
        )
        .is_ok()
        {
            dpi_x.max(96)
        } else {
            GetDpiForSystem().max(96)
        };
        MonitorMetrics {
            work_area,
            pixels_per_point: dpi as f32 / 96.0 * zoom_factor.max(0.5),
        }
    }
}

pub(super) fn popup_window() -> Option<HWND> {
    let title = HSTRING::from(super::VIEWPORT_TITLE);
    unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())).ok() }
        .filter(|window| !window.is_invalid())
}

pub(super) fn activate(window: HWND) {
    unsafe {
        let _ = SetForegroundWindow(window);
    }
}

pub(super) fn owns_foreground(window: HWND) -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        foreground == window || GetAncestor(foreground, GA_ROOT) == window
    }
}

pub(super) fn apply_bounds_and_region(window: HWND, placement: PopupPlacement, expanded: bool) {
    unsafe {
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            placement.physical_position.x,
            placement.physical_position.y,
            placement.physical_size[0],
            placement.physical_size[1],
            SWP_NOACTIVATE,
        );

        let scale = placement.physical_size[1] as f32 / MAIN_HEIGHT;
        let main_width = scale_value(MAIN_WIDTH, scale);
        let main_height = scale_value(MAIN_HEIGHT, scale);
        let radius = scale_value(16.0, scale).max(2);
        let combined = CreateRoundRectRgn(0, 0, main_width + 1, main_height + 1, radius, radius);

        if expanded && placement.has_flyout() {
            let flyout_left = scale_value(MAIN_WIDTH + FLYOUT_GAP, scale);
            let flyout_top = scale_value(placement.flyout_top, scale);
            let flyout_right = flyout_left + scale_value(FLYOUT_WIDTH, scale);
            let flyout_bottom = flyout_top + scale_value(placement.flyout_height, scale);
            let flyout = CreateRoundRectRgn(
                flyout_left,
                flyout_top,
                flyout_right + 1,
                flyout_bottom + 1,
                radius,
                radius,
            );
            let _ = CombineRgn(Some(combined), Some(combined), Some(flyout), RGN_OR);
            let _ = DeleteObject(flyout.into());
        }

        // Windows owns `combined` after a successful SetWindowRgn call.
        let _ = SetWindowRgn(window, Some(combined), true);
    }
}

fn scale_value(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}
