use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetShellWindow, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, WINDOWPLACEMENT, WS_CAPTION, WS_CHILD,
    WS_EX_NOREDIRECTIONBITMAP, WS_THICKFRAME,
};

use super::engine::MonitorInfo;

const MONITOR_EDGE_TOLERANCE_PX: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowCaptureEligibility {
    Supported,
    DisplayOnly,
}

pub(super) fn classify(hwnd: HWND) -> WindowCaptureEligibility {
    let Some(signals) = query_signals(hwnd) else {
        return WindowCaptureEligibility::Supported;
    };
    classify_signals(signals)
}

pub(super) fn monitor_requires_desktop_duplication(monitor: &MonitorInfo) -> bool {
    let selected_monitor = Bounds {
        left: monitor.x,
        top: monitor.y,
        right: monitor.x.saturating_add(monitor.width as i32),
        bottom: monitor.y.saturating_add(monitor.height as i32),
    };
    let current_process_id = unsafe { GetCurrentProcessId() };
    let shell_window = unsafe { GetShellWindow() };
    let Ok(windows) = windows_capture::window::Window::enumerate() else {
        return false;
    };

    windows.into_iter().any(|window| {
        if !window.is_valid() {
            return false;
        }
        let hwnd = HWND(window.as_raw_hwnd());
        if hwnd == shell_window || window.title().map_or(true, |title| title.trim().is_empty()) {
            return false;
        }
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return false;
        }
        let mut process_id = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        }
        if process_id == current_process_id {
            return false;
        }
        query_signals(hwnd)
            .is_some_and(|signals| is_monitor_filling_presentation(signals, selected_monitor))
    })
}

fn query_signals(hwnd: HWND) -> Option<WindowSignals> {
    let bounds = window_bounds(hwnd)?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        return None;
    }

    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool() {
        return None;
    }

    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 };
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    Some(WindowSignals {
        bounds: Bounds::from(bounds),
        monitor: Bounds::from(monitor_info.rcMonitor),
        style,
        ex_style,
    })
}

fn window_bounds(hwnd: HWND) -> Option<RECT> {
    if unsafe { IsIconic(hwnd).as_bool() } {
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        if unsafe { GetWindowPlacement(hwnd, &mut placement) }.is_ok() {
            let bounds = placement.rcNormalPosition;
            if bounds.right > bounds.left && bounds.bottom > bounds.top {
                return Some(bounds);
            }
        }
    }

    let mut bounds = RECT::default();
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut bounds as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_err()
        && unsafe { GetWindowRect(hwnd, &mut bounds) }.is_err()
    {
        return None;
    }

    (bounds.right > bounds.left && bounds.bottom > bounds.top).then_some(bounds)
}

fn classify_signals(signals: WindowSignals) -> WindowCaptureEligibility {
    let has_no_redirection_surface = signals.ex_style & WS_EX_NOREDIRECTIONBITMAP.0 != 0;
    if has_no_redirection_surface || is_monitor_filling_presentation(signals, signals.monitor) {
        WindowCaptureEligibility::DisplayOnly
    } else {
        WindowCaptureEligibility::Supported
    }
}

fn is_monitor_filling_presentation(signals: WindowSignals, monitor: Bounds) -> bool {
    let has_no_redirection_surface = signals.ex_style & WS_EX_NOREDIRECTIONBITMAP.0 != 0;
    let is_top_level = signals.style & WS_CHILD.0 == 0;
    let has_standard_frame = signals.style & (WS_CAPTION.0 | WS_THICKFRAME.0) != 0;
    !has_no_redirection_surface
        && is_top_level
        && !has_standard_frame
        && signals.bounds.matches(monitor, MONITOR_EDGE_TOLERANCE_PX)
}

#[derive(Clone, Copy, Debug)]
struct WindowSignals {
    bounds: Bounds,
    monitor: Bounds,
    style: u32,
    ex_style: u32,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Bounds {
    fn matches(self, other: Self, tolerance: i32) -> bool {
        edge_delta(self.left, other.left) <= tolerance
            && edge_delta(self.top, other.top) <= tolerance
            && edge_delta(self.right, other.right) <= tolerance
            && edge_delta(self.bottom, other.bottom) <= tolerance
    }
}

impl From<RECT> for Bounds {
    fn from(value: RECT) -> Self {
        Self {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
        }
    }
}

fn edge_delta(left: i32, right: i32) -> i32 {
    (i64::from(left) - i64::from(right)).unsigned_abs() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Bounds = Bounds {
        left: 0,
        top: 0,
        right: 2560,
        bottom: 1080,
    };

    fn classify(bounds: Bounds, style: u32, ex_style: u32) -> WindowCaptureEligibility {
        classify_signals(WindowSignals {
            bounds,
            monitor: MONITOR,
            style,
            ex_style,
        })
    }

    #[test]
    fn monitor_filling_borderless_surface_is_display_only() {
        assert_eq!(
            classify(MONITOR, 0x9602_0000, 0),
            WindowCaptureEligibility::DisplayOnly
        );
    }

    #[test]
    fn maximized_work_area_window_remains_supported() {
        let work_area = Bounds {
            bottom: 1032,
            ..MONITOR
        };
        assert_eq!(
            classify(work_area, WS_CAPTION.0 | WS_THICKFRAME.0, 0),
            WindowCaptureEligibility::Supported
        );
    }

    #[test]
    fn framed_monitor_filling_window_remains_supported() {
        assert_eq!(
            classify(MONITOR, WS_CAPTION.0 | WS_THICKFRAME.0, 0),
            WindowCaptureEligibility::Supported
        );
    }

    #[test]
    fn no_redirection_surface_is_display_only_at_any_size() {
        let windowed = Bounds {
            left: 100,
            top: 100,
            right: 1100,
            bottom: 800,
        };
        assert_eq!(
            classify(windowed, WS_CAPTION.0, WS_EX_NOREDIRECTIONBITMAP.0),
            WindowCaptureEligibility::DisplayOnly
        );
    }

    #[test]
    fn no_redirection_monitor_surface_does_not_force_the_display_backend() {
        let signals = WindowSignals {
            bounds: MONITOR,
            monitor: MONITOR,
            style: 0x9400_0000,
            ex_style: WS_EX_NOREDIRECTIONBITMAP.0,
        };
        assert!(!is_monitor_filling_presentation(signals, MONITOR));
    }
}
