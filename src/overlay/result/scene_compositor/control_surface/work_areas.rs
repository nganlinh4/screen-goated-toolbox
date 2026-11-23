//! Monitor bounds and usable areas in the compositor's physical coordinate space.
use serde::Serialize;
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONULL, MONITORINFO,
    MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_PER_MONITOR_AWARE, GetAwarenessFromDpiAwarenessContext, GetDpiForWindow,
    GetWindowDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, w};

const MAX_MONITORS: usize = 64;
static MONITORS: LazyLock<Mutex<Vec<MonitorArea>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Clone, Debug, Serialize)]
struct MonitorArea {
    bounds: [i32; 4],
    work: [i32; 4],
    dpi: u32,
}

struct MonitorCollection {
    origin: (i32, i32),
    monitors: Vec<(HMONITOR, RECT, RECT)>,
}

fn relative_rect(rect: RECT, origin: (i32, i32)) -> [i32; 4] {
    [
        rect.left.saturating_sub(origin.0),
        rect.top.saturating_sub(origin.1),
        rect.right.saturating_sub(rect.left).max(0),
        rect.bottom.saturating_sub(rect.top).max(0),
    ]
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    unsafe {
        let collection = &mut *(data.0 as *mut MonitorCollection);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if collection.monitors.len() < MAX_MONITORS && GetMonitorInfoW(monitor, &mut info).as_bool()
        {
            collection
                .monitors
                .push((monitor, info.rcMonitor, info.rcWork));
        }
        BOOL(1)
    }
}

pub(in crate::overlay::result::scene_compositor) fn script(origin: (i32, i32)) -> String {
    let mut collection = MonitorCollection {
        origin,
        monitors: Vec::new(),
    };
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut collection as *mut _ as isize),
        );
    }
    let areas: Vec<_> = collection
        .monitors
        .into_iter()
        .map(|(monitor, bounds, work)| MonitorArea {
            bounds: relative_rect(bounds, collection.origin),
            work: relative_rect(work, collection.origin),
            dpi: probe_monitor_dpi(monitor, bounds).unwrap_or(96),
        })
        .collect();
    *MONITORS.lock().unwrap_or_else(|error| error.into_inner()) = areas.clone();
    format!(
        "window.__SGT_SET_CONTROL_MONITORS__?.({});",
        serde_json::to_string(&areas).expect("monitor rectangles serialize")
    )
}

fn probe_monitor_dpi(monitor: HMONITOR, bounds: RECT) -> Option<u32> {
    let width = i64::from(bounds.right) - i64::from(bounds.left);
    let height = i64::from(bounds.bottom) - i64::from(bounds.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    let x = i32::try_from(i64::from(bounds.left) + (width - 1) / 2).ok()?;
    let y = i32::try_from(i64::from(bounds.top) + (height - 1) / 2).ok()?;
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0),
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            x,
            y,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .ok()?;
        let same_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) == monitor;
        let aware = GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(hwnd))
            == DPI_AWARENESS_PER_MONITOR_AWARE;
        let dpi = GetDpiForWindow(hwnd);
        let _ = DestroyWindow(hwnd);
        (same_monitor && aware && dpi != 0).then_some(dpi)
    }
}

pub(in crate::overlay::result::scene_compositor) fn dpi_for_rect(
    rect: &super::super::protocol::SceneRect,
) -> u32 {
    let monitors = MONITORS.lock().unwrap_or_else(|error| error.into_inner());
    selected_dpi(&monitors, [rect.x, rect.y, rect.width, rect.height])
}

fn selected_dpi(monitors: &[MonitorArea], rect: [i32; 4]) -> u32 {
    let x = i64::from(rect[0]) + i64::from(rect[2].max(1)) - 1;
    let y = i64::from(rect[1]) + i64::from(rect[3].max(1)) - 1;
    let valid =
        |m: &&MonitorArea| m.bounds[2] > 0 && m.bounds[3] > 0 && m.work[2] > 0 && m.work[3] > 0;
    if let Some(m) = monitors.iter().filter(valid).find(|m| {
        let left = i64::from(m.bounds[0]);
        let top = i64::from(m.bounds[1]);
        x >= left
            && x < left + i64::from(m.bounds[2])
            && y >= top
            && y < top + i64::from(m.bounds[3])
    }) {
        return m.dpi;
    }
    monitors
        .iter()
        .filter(valid)
        .min_by_key(|m| {
            let left = i64::from(m.bounds[0]);
            let top = i64::from(m.bounds[1]);
            let right = left + i64::from(m.bounds[2]);
            let bottom = top + i64::from(m.bounds[3]);
            let dx = (left - x).max(0).max(x - right);
            let dy = (top - y).max(0).max(y - bottom);
            i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)
        })
        .map_or(96, |m| m.dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_desktop_origins_preserve_taskbar_insets() {
        assert_eq!(
            relative_rect(
                RECT {
                    left: -1600,
                    top: -200,
                    right: 0,
                    bottom: 760
                },
                (-1600, -200)
            ),
            [0, 0, 1600, 960]
        );
        assert_eq!(
            relative_rect(
                RECT {
                    left: -1560,
                    top: -200,
                    right: 0,
                    bottom: 720
                },
                (-1600, -200)
            ),
            [40, 0, 1560, 920]
        );
    }

    #[test]
    fn dpi_selection_follows_card_bottom_right_and_nearest_monitor() {
        let areas = vec![
            MonitorArea {
                bounds: [0, 0, 100, 100],
                work: [0, 0, 100, 100],
                dpi: 96,
            },
            MonitorArea {
                bounds: [100, 0, 100, 100],
                work: [100, 0, 100, 100],
                dpi: 192,
            },
        ];
        assert_eq!(selected_dpi(&areas, [90, 10, 10, 20]), 96);
        assert_eq!(selected_dpi(&areas, [90, 10, 11, 20]), 192);
        assert_eq!(selected_dpi(&areas, [250, 10, 20, 20]), 192);
        assert_eq!(selected_dpi(&[], [0, 0, 1, 1]), 96);
        assert_eq!(selected_dpi(&areas, [i32::MAX, i32::MIN, i32::MAX, 1]), 192);
        let mut invalid = areas.clone();
        invalid[1].work[2] = 0;
        assert_eq!(selected_dpi(&invalid, [100, 10, 5, 5]), 96);
        let negative = [MonitorArea {
            bounds: [-200, -100, 100, 100],
            work: [-200, -100, 100, 100],
            dpi: 144,
        }];
        assert_eq!(selected_dpi(&negative, [-150, -60, 30, 20]), 144);
    }
}
