//! Physical-pixel placement within the usable area of a Windows monitor.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITOR_FROM_FLAGS,
    MONITORINFO, MonitorFromPoint, MonitorFromRect, MonitorFromWindow,
};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WorkArea {
    fn from_rect(rect: RECT) -> Self {
        Self {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(1),
            height: (rect.bottom - rect.top).max(1),
        }
    }

    pub fn centered(self, desired_width: i32, desired_height: i32) -> RECT {
        let width = desired_width.clamp(1, self.width);
        let height = desired_height.clamp(1, self.height);
        RECT {
            left: self.x + (self.width - width) / 2,
            top: self.y + (self.height - height) / 2,
            right: self.x + (self.width + width) / 2,
            bottom: self.y + (self.height + height) / 2,
        }
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub fn fit(self, x: i32, y: i32, desired_width: i32, desired_height: i32) -> RECT {
        let width = desired_width.clamp(1, self.width);
        let height = desired_height.clamp(1, self.height);
        let left = x.clamp(self.x, self.x + self.width - width);
        let top = y.clamp(self.y, self.y + self.height - height);
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub fn near_point(
        self,
        point: POINT,
        offset_x: i32,
        offset_y: i32,
        width: i32,
        height: i32,
    ) -> RECT {
        let x = point.x + offset_x;
        let y = point.y + offset_y;
        let x = if x + width > self.x + self.width {
            point.x - width - offset_x.abs()
        } else {
            x
        };
        let y = if y + height > self.y + self.height {
            point.y - height - offset_y.abs()
        } else {
            y
        };
        self.fit(x, y, width, height)
    }
}

pub(crate) fn primary_work_area() -> WorkArea {
    monitor_work_area(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn work_area_at_point(point: POINT) -> WorkArea {
    monitor_work_area(point, MONITOR_DEFAULTTONEAREST)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn fit_rect_to_work_area(rect: RECT) -> RECT {
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
    area_for_monitor(monitor).fit(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn cursor_work_area() -> (POINT, WorkArea) {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    let area = monitor_work_area(point, MONITOR_DEFAULTTONEAREST);
    (point, area)
}

pub(crate) fn window_work_area(hwnd: HWND) -> WorkArea {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    area_for_monitor(monitor)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn primary_scale() -> f64 {
    monitor_scale(unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) })
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn scale_at_point(point: POINT) -> f64 {
    monitor_scale(unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) })
}

#[cfg(not(feature = "recorder-worker"))]
fn monitor_scale(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> f64 {
    let mut x = 96;
    let mut y = 96;
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.is_ok() {
        (f64::from(x) / 96.0).max(1.0)
    } else {
        1.0
    }
}

fn monitor_work_area(point: POINT, fallback: MONITOR_FROM_FLAGS) -> WorkArea {
    area_for_monitor(unsafe { MonitorFromPoint(point, fallback) })
}

fn area_for_monitor(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> WorkArea {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            WorkArea::from_rect(info.rcWork)
        } else {
            WorkArea {
                x: 0,
                y: 0,
                width: GetSystemMetrics(SM_CXSCREEN).max(1),
                height: GetSystemMetrics(SM_CYSCREEN).max(1),
            }
        }
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn placement_stays_in_negative_origin_and_small_work_areas() {
        let area = WorkArea {
            x: -1366,
            y: -120,
            width: 1366,
            height: 680,
        };
        assert_eq!(
            area.centered(1600, 900),
            RECT {
                left: -1366,
                top: -120,
                right: 0,
                bottom: 560
            }
        );
        assert_eq!(
            area.fit(-2000, 900, 450, 70),
            RECT {
                left: -1366,
                top: 490,
                right: -916,
                bottom: 560
            }
        );
    }

    #[test]
    fn cursor_badge_flips_at_right_and_bottom_edges() {
        let area = WorkArea {
            x: 0,
            y: 0,
            width: 1280,
            height: 680,
        };
        assert_eq!(
            area.near_point(POINT { x: 1260, y: 660 }, -20, 24, 240, 140),
            RECT {
                left: 1000,
                top: 496,
                right: 1240,
                bottom: 636
            }
        );
    }

    #[test]
    fn saved_window_geometry_is_bounded_after_display_change() {
        let area = WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1020,
        };
        assert_eq!(
            area.fit(2400, 800, 2400, 1200),
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1020,
            }
        );
    }
}
