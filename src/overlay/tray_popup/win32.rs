use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Dwm::{
    DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
    DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetCursorPos, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowThreadProcessId, HWND_TOPMOST, SET_WINDOW_POS_FLAGS,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WS_BORDER, WS_CAPTION,
    WS_DLGFRAME, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};
use windows::core::{HSTRING, PCWSTR};

use super::layout::{
    FLYOUT_GAP, FLYOUT_WIDTH, MAIN_HEIGHT, MAIN_WIDTH, PhysicalPoint, PopupPlacement, WorkArea,
};

const OFFSCREEN_COORDINATE: i32 = -32_000;

pub(super) struct MonitorMetrics {
    pub work_area: WorkArea,
    pub pixels_per_point: f32,
}

pub(super) struct WindowIdentity {
    pub hwnd: usize,
    pub pid: u32,
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
    let title = HSTRING::from(super::viewport_title());
    unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())).ok() }
        .filter(|window| !window.is_invalid())
}

pub(super) fn foreground_identity() -> WindowIdentity {
    unsafe {
        let window = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        WindowIdentity {
            hwnd: window.0 as usize,
            pid,
        }
    }
}

pub(super) fn prepare_offscreen(window: HWND, placement: PopupPlacement) {
    unsafe {
        configure_borderless_popup(window);
        set_bounds_and_region(
            window,
            OFFSCREEN_COORDINATE,
            OFFSCREEN_COORDINATE,
            placement,
            false,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

pub(super) fn reveal(window: HWND, placement: PopupPlacement) {
    unsafe {
        configure_borderless_popup(window);
        set_bounds_and_region(
            window,
            placement.physical_position.x,
            placement.physical_position.y,
            placement,
            false,
            SWP_NOACTIVATE,
        );
        let _ = SetForegroundWindow(window);
        crate::log_info!(
            "[TrayPopup] reveal hwnd={:#x} mode=immediate",
            window.0 as usize
        );
    }
}

pub(super) fn hide(window: HWND) {
    unsafe {
        // Keep the swapchain alive and retained. A later tray click only moves
        // this already-painted window; it never asks DWM to show or uncloak it.
        let _ = SetWindowPos(
            window,
            None,
            OFFSCREEN_COORDINATE,
            OFFSCREEN_COORDINATE,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        );
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
        configure_borderless_popup(window);
        set_bounds_and_region(
            window,
            placement.physical_position.x,
            placement.physical_position.y,
            placement,
            expanded,
            SWP_NOACTIVATE,
        );
    }
}

unsafe fn set_bounds_and_region(
    window: HWND,
    x: i32,
    y: i32,
    placement: PopupPlacement,
    expanded: bool,
    flags: SET_WINDOW_POS_FLAGS,
) {
    unsafe {
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            x,
            y,
            placement.physical_size[0],
            placement.physical_size[1],
            flags,
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
        disable_non_client_frame(window);
    }
}

unsafe fn configure_borderless_popup(window: HWND) {
    unsafe {
        let frame_styles = WS_CAPTION
            | WS_THICKFRAME
            | WS_BORDER
            | WS_DLGFRAME
            | WS_SYSMENU
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX;
        let style = WINDOW_STYLE(GetWindowLongPtrW(window, GWL_STYLE) as u32);
        let borderless = (style & !frame_styles) | WS_POPUP;

        let edge_styles =
            WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE;
        let ex_style = WINDOW_EX_STYLE(GetWindowLongPtrW(window, GWL_EXSTYLE) as u32);
        let borderless_ex = ex_style & !edge_styles;

        if borderless != style {
            let _ = SetWindowLongPtrW(window, GWL_STYLE, borderless.0 as isize);
        }
        if borderless_ex != ex_style {
            let _ = SetWindowLongPtrW(window, GWL_EXSTYLE, borderless_ex.0 as isize);
        }
        if borderless != style || borderless_ex != ex_style {
            let _ = SetWindowPos(
                window,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }

        let transitions_disabled = 1i32;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            std::ptr::addr_of!(transitions_disabled).cast(),
            std::mem::size_of_val(&transitions_disabled) as u32,
        );
        disable_non_client_frame(window);
    }
}

fn disable_non_client_frame(window: HWND) {
    unsafe {
        let policy = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_NCRENDERING_POLICY,
            std::ptr::addr_of!(policy).cast(),
            std::mem::size_of_val(&policy) as u32,
        );
        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_BORDER_COLOR,
            std::ptr::addr_of!(border_color).cast(),
            std::mem::size_of_val(&border_color) as u32,
        );
        let corner_preference = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_preference).cast(),
            std::mem::size_of_val(&corner_preference) as u32,
        );
    }
}

fn scale_value(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}
