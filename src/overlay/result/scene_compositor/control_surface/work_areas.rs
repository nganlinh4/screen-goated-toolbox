//! Monitor bounds and usable areas in the compositor's physical coordinate space.
use serde::Serialize;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::BOOL;

#[derive(Serialize)]
struct MonitorArea {
    bounds: [i32; 4],
    work: [i32; 4],
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
        let areas = &mut *(data.0 as *mut Vec<MonitorArea>);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let origin = (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            );
            areas.push(MonitorArea {
                bounds: relative_rect(info.rcMonitor, origin),
                work: relative_rect(info.rcWork, origin),
            });
        }
        BOOL(1)
    }
}

pub(in crate::overlay::result::scene_compositor) fn script() -> String {
    let mut areas: Vec<MonitorArea> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut areas as *mut _ as isize),
        );
    }
    format!(
        "window.__SGT_SET_CONTROL_MONITORS__?.({});",
        serde_json::to_string(&areas).expect("monitor rectangles serialize")
    )
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
}
