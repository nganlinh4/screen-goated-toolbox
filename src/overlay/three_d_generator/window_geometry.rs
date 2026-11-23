//! DPI-aware geometry and persistence for the desktop creation window.

use crate::overlay::display_placement::WorkArea;
use std::io::Read;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

const BASE_DPI: u32 = 96;
const MIN_WIDTH: i32 = 840;
const MIN_HEIGHT: i32 = 540;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedWindowSize {
    width: i32,
    height: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logical_width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logical_height: Option<i32>,
}

thread_local! {
    static INTERACTIVE_SIZE: std::cell::Cell<Option<(i32, i32)>> = const { std::cell::Cell::new(None) };
    static USER_SIZED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn scale(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi.max(1)) + 48) / 96).clamp(1, i64::from(i32::MAX)) as i32
}

fn unscale(value: i32, dpi: u32) -> i32 {
    let dpi = i64::from(dpi.max(1));
    ((i64::from(value) * 96 + dpi / 2) / dpi).clamp(1, i64::from(i32::MAX)) as i32
}

pub(super) fn dpi(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => BASE_DPI,
        value => value,
    }
}

pub(super) fn minimum(area: WorkArea, dpi: u32) -> (i32, i32) {
    (
        scale(MIN_WIDTH, dpi).min(area.width),
        scale(MIN_HEIGHT, dpi).min(area.height),
    )
}

pub(super) fn border(hwnd: HWND) -> i32 {
    scale(7, dpi(hwnd))
}

fn initial_rect(area: WorkArea, dpi: u32, saved: Option<&SavedWindowSize>) -> RECT {
    let defaults = (
        ((area.width as f64 * 0.58) as i32).clamp(scale(920, dpi), scale(1240, dpi)),
        ((area.height as f64 * 0.62) as i32).clamp(scale(600, dpi), scale(820, dpi)),
    );
    let desired = saved
        .and_then(|size| {
            if size.width <= 0 || size.height <= 0 {
                return None;
            }
            match (size.logical_width, size.logical_height) {
                (Some(width), Some(height)) if width > 0 && height > 0 => {
                    Some((scale(width, dpi), scale(height, dpi)))
                }
                _ => Some((size.width, size.height)), // legacy physical pixels
            }
        })
        .unwrap_or(defaults);
    let (min_width, min_height) = minimum(area, dpi);
    area.centered(
        desired.0.clamp(min_width, area.width),
        desired.1.clamp(min_height, area.height),
    )
}

fn size_path() -> std::path::PathBuf {
    crate::paths::app_runtime_local_data_dir().join("3d-generator-window.json")
}

fn read_saved(reader: impl Read) -> Option<SavedWindowSize> {
    let mut bytes = Vec::new();
    reader.take(16 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= 16 * 1024)
        .then(|| serde_json::from_slice::<SavedWindowSize>(&bytes).ok())
        .flatten()
}

pub(super) fn initial(hwnd: HWND, area: WorkArea) -> RECT {
    let path = size_path();
    let saved = std::fs::File::open(path).ok().and_then(read_saved);
    initial_rect(area, dpi(hwnd), saved.as_ref())
}

fn physical_size(hwnd: HWND) -> Option<(i32, i32)> {
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect).ok()?;
    }
    Some((rect.right - rect.left, rect.bottom - rect.top))
}

pub(super) fn begin_user_resize(hwnd: HWND) {
    INTERACTIVE_SIZE.with(|size| size.set(physical_size(hwnd)));
    USER_SIZED.with(|sized| sized.set(false));
}

pub(super) fn mark_user_resize() {
    USER_SIZED.with(|sized| sized.set(true));
}

pub(super) fn finish_user_resize(hwnd: HWND) {
    let before = INTERACTIVE_SIZE.with(|size| size.take());
    if !USER_SIZED.with(|sized| sized.replace(false)) {
        return;
    }
    let Some(after) = physical_size(hwnd) else {
        return;
    };
    if before.is_none() || before == Some(after) || after.0 <= 0 || after.1 <= 0 {
        return;
    }
    let dpi = dpi(hwnd);
    let size = SavedWindowSize {
        width: after.0,
        height: after.1,
        logical_width: Some(unscale(after.0, dpi)),
        logical_height: Some(unscale(after.1, dpi)),
    };
    let path = size_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        let _ = crate::atomic_json::write_json_atomic(&path, &size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: i32, height: i32) -> WorkArea {
        WorkArea {
            x: -1920,
            y: 50,
            width,
            height,
        }
    }
    fn dimensions(rect: RECT) -> (i32, i32) {
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    #[test]
    fn scaled_defaults_and_minima_fit_the_monitor() {
        for dpi in [96, 144, 192] {
            let work = area(3840, 2160);
            let rect = initial_rect(work, dpi, None);
            let (width, height) = dimensions(rect);
            let (min_width, min_height) = minimum(work, dpi);
            assert!(width >= min_width && height >= min_height);
            assert!(rect.left >= work.x && rect.top >= work.y);
            assert!(rect.right <= work.x + work.width && rect.bottom <= work.y + work.height);
        }
        assert_eq!(
            dimensions(initial_rect(area(800, 450), 192, None)),
            (800, 450)
        );
    }

    #[test]
    fn legacy_pixels_and_new_dips_are_distinguished_without_guessing() {
        let legacy: SavedWindowSize =
            serde_json::from_str(r#"{"width":1500,"height":900}"#).unwrap();
        let logical: SavedWindowSize = serde_json::from_str(
            r#"{"width":1500,"height":900,"logicalWidth":1000,"logicalHeight":600}"#,
        )
        .unwrap();
        let work = area(3840, 2160);
        assert_eq!(
            dimensions(initial_rect(work, 144, Some(&legacy))),
            (1500, 900)
        );
        assert_eq!(
            dimensions(initial_rect(work, 144, Some(&logical))),
            (1500, 900)
        );
        assert_eq!(
            dimensions(initial_rect(work, 192, Some(&logical))),
            (2000, 1200)
        );
        assert_eq!(unscale(scale(1000, 144), 144), 1000);
    }

    #[test]
    fn malformed_and_oversized_saved_sizes_are_rejected() {
        assert!(read_saved(&b"{broken"[..]).is_none());
        assert!(read_saved(std::io::repeat(b' ').take(16 * 1024 + 2)).is_none());
    }
}
