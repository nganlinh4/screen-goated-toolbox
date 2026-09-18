//! Monitor-relative capture using the ordinary Screen Translate processing target.
use crate::config::types::ScreenTranslateRegion;
use anyhow::{Context, Result, ensure};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

#[derive(Clone)]
struct Monitor {
    name: String,
    bounds: RECT,
}

pub(crate) type RegionEditReceiver =
    std::sync::mpsc::Receiver<Result<Option<ScreenTranslateRegion>>>;

unsafe extern "system" fn collect(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> windows::core::BOOL {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
            let count = info
                .szDevice
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(info.szDevice.len());
            let monitors = &mut *(data.0 as *mut Vec<Monitor>);
            monitors.push(Monitor {
                name: String::from_utf16_lossy(&info.szDevice[..count]),
                bounds: info.monitorInfo.rcMonitor,
            });
        }
        true.into()
    }
}

fn resolve(saved: Option<&ScreenTranslateRegion>, editing: bool) -> Result<(Monitor, RECT)> {
    let mut monitors = Vec::<Monitor>::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor)?;
    }
    ensure!(
        editing || saved.is_none_or(|r| monitors.iter().any(|m| m.name == r.monitor)),
        "saved display is unavailable; adjust the capture region"
    );
    let monitor = saved
        .and_then(|r| monitors.iter().find(|m| m.name == r.monitor))
        .or_else(|| {
            monitors.iter().find(|m| {
                cursor.x >= m.bounds.left
                    && cursor.x < m.bounds.right
                    && cursor.y >= m.bounds.top
                    && cursor.y < m.bounds.bottom
            })
        })
        .or_else(|| monitors.first())
        .context("no display is available")?
        .clone();
    ensure!(
        monitor.bounds.right - monitor.bounds.left >= 64
            && monitor.bounds.bottom - monitor.bounds.top >= 64,
        "display is too small for region selection"
    );
    // A disconnected saved monitor must never capture unrelated coordinates.
    let rect = saved
        .filter(|r| r.monitor == monitor.name)
        .map(|r| decode(r.edges, monitor.bounds))
        .unwrap_or(monitor.bounds);
    Ok((monitor, rect))
}

fn decode(edges: [u16; 4], bounds: RECT) -> RECT {
    let [l, t, r, b] = edges;
    if l >= r || t >= b || r > 10_000 || b > 10_000 {
        return bounds;
    }
    let x = |v| {
        bounds.left
            + ((i64::from(bounds.right - bounds.left) * i64::from(v) + 5_000) / 10_000) as i32
    };
    let y = |v| {
        bounds.top
            + ((i64::from(bounds.bottom - bounds.top) * i64::from(v) + 5_000) / 10_000) as i32
    };
    let left = x(l).min(bounds.right - 1);
    let top = y(t).min(bounds.bottom - 1);
    RECT {
        left,
        top,
        right: x(r).max(left + 1),
        bottom: y(b).max(top + 1),
    }
}

fn encode(rect: RECT, monitor: &Monitor) -> ScreenTranslateRegion {
    let edge = |value: i32, origin: i32, size: i32| {
        ((i64::from(value - origin) * 10_000 + i64::from(size) / 2) / i64::from(size))
            .clamp(0, 10_000) as u16
    };
    let b = monitor.bounds;
    ScreenTranslateRegion {
        monitor: monitor.name.clone(),
        edges: [
            edge(rect.left, b.left, b.right - b.left),
            edge(rect.top, b.top, b.bottom - b.top),
            edge(rect.right, b.left, b.right - b.left),
            edge(rect.bottom, b.top, b.bottom - b.top),
        ],
    }
}

pub(crate) fn translate() {
    if !crate::overlay::try_claim_capture() {
        return;
    }
    std::thread::spawn(|| {
        let result = (|| -> Result<()> {
            let saved = crate::APP
                .lock()
                .ok()
                .and_then(|app| app.config.screen_translate.fixed_region.clone());
            let (_, rect) = resolve(saved.as_ref(), false)?;
            let capture = crate::screen_capture::capture_screen_fast()?;
            let crop = unsafe {
                RECT {
                    left: rect.left - GetSystemMetrics(SM_XVIRTUALSCREEN),
                    top: rect.top - GetSystemMetrics(SM_YVIRTUALSCREEN),
                    right: rect.right - GetSystemMetrics(SM_XVIRTUALSCREEN),
                    bottom: rect.bottom - GetSystemMetrics(SM_YVIRTUALSCREEN),
                }
            };
            ensure!(
                crop.left >= 0
                    && crop.top >= 0
                    && crop.right <= capture.width
                    && crop.bottom <= capture.height,
                "display layout changed; retry capture"
            );
            let image = crate::overlay::selection::extract_crop_from_hbitmap_public(&capture, crop);
            // Freeze the source before first-use progress can appear on screen.
            super::prepare_detector();
            super::capture_target().process(image, rect);
            Ok(())
        })();
        crate::overlay::set_is_busy(false);
        if let Err(error) = result {
            super::capture::notify_error(&error.to_string());
        }
    });
}

pub(crate) fn edit(
    saved: Option<ScreenTranslateRegion>,
    ctx: eframe::egui::Context,
) -> RegionEditReceiver {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<Option<ScreenTranslateRegion>> {
            ensure!(
                crate::overlay::try_claim_capture(),
                "another screen selection is active"
            );
            let result = (|| {
                // Wait for the compositor to present the already-hidden settings window.
                unsafe {
                    windows::Win32::Graphics::Dwm::DwmFlush()?;
                }
                let (monitor, rect) = resolve(saved.as_ref(), true)?;
                let capture = crate::screen_capture::capture_screen_fast()?;
                crate::APP
                    .lock()
                    .map_err(|_| anyhow::anyhow!("settings unavailable"))?
                    .screenshot_handle = Some(capture);
                let selected = crate::overlay::selection::edit_region(rect, monitor.bounds);
                if let Ok(mut app) = crate::APP.lock() {
                    app.screenshot_handle = None;
                }
                Ok(selected?.map(|rect| encode(rect, &monitor)))
            })();
            crate::overlay::set_is_busy(false);
            result
        })();
        if send.send(result).is_ok() {
            crate::gui::app::signal_restore_window();
        }
        ctx.request_repaint();
    });
    receive
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_tracks_monitor_origin_and_resolution() {
        let bounds = RECT {
            left: -1920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let rect = decode([0, 1000, 10000, 9000], bounds);
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (-1920, -92, 0, 772)
        );
        assert_eq!(
            encode(
                rect,
                &Monitor {
                    name: "display".into(),
                    bounds
                }
            )
            .edges,
            [0, 1000, 10000, 9000]
        );
        let resized = decode(
            [0, 1000, 10000, 9000],
            RECT {
                left: 0,
                top: 0,
                right: 2560,
                bottom: 1440,
            },
        );
        assert_eq!((resized.top, resized.bottom), (144, 1296));
    }
    #[test]
    fn malformed_or_tiny_regions_remain_valid() {
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 600,
        };
        assert_eq!(decode([9000, 0, 2000, 10000], bounds), bounds);
        let tiny = decode([1, 1, 2, 2], bounds);
        assert!(tiny.right > tiny.left && tiny.bottom > tiny.top);
    }
}
