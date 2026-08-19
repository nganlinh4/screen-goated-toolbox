use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetSystemMetrics, GetWindowPlacement, GetWindowRect, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWMAXIMIZED, WINDOWPLACEMENT,
    WPF_RESTORETOMAXIMIZED,
};

/// Bumped to 2 when the stored size changed meaning: v1 wrote the *outer*
/// window rect in *physical pixels* but restored it through `with_inner_size`,
/// which reads *logical points*. Old files are discarded rather than reopening
/// a window a frame-and-a-scale-factor too large.
const SCHEMA_VERSION: u32 = 2;

/// Points per inch that Windows calls 100% scaling.
const DEFAULT_DPI: f32 = 96.0;
const MAX_STATE_BYTES: u64 = 4096;
const MIN_VISIBLE_EDGE: i32 = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MainWindowState {
    schema_version: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    maximized: bool,
}

/// Whether [`restore`] applied a saved geometry to this launch.
static RESTORED_GEOMETRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The client size [`restore`] applied, in points.
static RESTORED_SIZE: std::sync::Mutex<Option<(f32, f32)>> = std::sync::Mutex::new(None);

/// The size this launch reopened at, if it reopened at the user's own.
pub(crate) fn restored_inner_size() -> Option<(f32, f32)> {
    RESTORED_SIZE.lock().ok().and_then(|size| *size)
}

/// True when this launch reopened at the user's own size and position.
///
/// Startup then leaves the window alone; the default size and the centring are
/// for a first run, not for every run.
pub(crate) fn restored_geometry() -> bool {
    RESTORED_GEOMETRY.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn restore(viewport: eframe::egui::ViewportBuilder) -> eframe::egui::ViewportBuilder {
    let Some(state) = load().filter(|state| is_visible(*state, virtual_screen())) else {
        return viewport;
    };
    RESTORED_GEOMETRY.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut size) = RESTORED_SIZE.lock() {
        *size = Some((state.width as f32, state.height as f32));
    }
    viewport
        .with_position([state.x as f32, state.y as f32])
        .with_inner_size([state.width as f32, state.height as f32])
        .with_maximized(state.maximized)
}

/// Persists the main window's restored geometry.
///
/// Safe to call from any quit path: it no-ops when the main window is gone.
pub(crate) fn save_main_window() {
    let hwnd = unsafe { crate::gui::app::main_window_hwnd() };
    if hwnd.is_invalid() {
        return;
    }
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    if unsafe { GetWindowPlacement(hwnd, &mut placement) }.is_err() {
        return;
    }
    // `rcNormalPosition` is the restored *outer* rect in physical pixels;
    // `restore` feeds it to `with_inner_size`, which wants the *client* area in
    // logical points. Measure the frame this window actually has and divide by
    // its scale, or every launch reopens bigger than the last.
    let rect = placement.rcNormalPosition;
    let scale = window_scale(hwnd);
    let (frame_width, frame_height) = window_frame_size(hwnd);
    let state = MainWindowState {
        schema_version: SCHEMA_VERSION,
        x: to_points(rect.left, scale),
        y: to_points(rect.top, scale),
        width: to_points(
            rect.right
                .saturating_sub(rect.left)
                .saturating_sub(frame_width),
            scale,
        ),
        height: to_points(
            rect.bottom
                .saturating_sub(rect.top)
                .saturating_sub(frame_height),
            scale,
        ),
        maximized: placement.showCmd == SW_SHOWMAXIMIZED.0 as u32
            || placement.flags.contains(WPF_RESTORETOMAXIMIZED),
    };
    if !is_visible(state, virtual_screen()) {
        return;
    }
    let path = state_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        let _ = crate::atomic_json::write_json_atomic(&path, &state);
    }
}

/// Physical pixels this window spends on its non-client frame.
fn window_frame_size(hwnd: windows::Win32::Foundation::HWND) -> (i32, i32) {
    let mut outer = RECT::default();
    let mut client = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut outer) }.is_err()
        || unsafe { GetClientRect(hwnd, &mut client) }.is_err()
    {
        return (0, 0);
    }
    (
        (outer.right - outer.left) - (client.right - client.left),
        (outer.bottom - outer.top) - (client.bottom - client.top),
    )
}

fn window_scale(hwnd: windows::Win32::Foundation::HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / DEFAULT_DPI
    }
}

fn to_points(physical: i32, scale: f32) -> i32 {
    if scale <= 0.0 {
        return physical;
    }
    (physical as f32 / scale).round() as i32
}

fn load() -> Option<MainWindowState> {
    let path = state_path();
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES {
        return None;
    }
    let state: MainWindowState = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (state.schema_version == SCHEMA_VERSION).then_some(state)
}

fn state_path() -> std::path::PathBuf {
    crate::paths::app_local_data_dir().join("main-window-state.json")
}

fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn is_visible(state: MainWindowState, screen: (i32, i32, i32, i32)) -> bool {
    if state.schema_version != SCHEMA_VERSION
        || state.width < crate::MIN_WINDOW_WIDTH as i32
        || state.height < crate::MIN_WINDOW_HEIGHT as i32
        || state.width > 32_768
        || state.height > 32_768
        || screen.2 <= 0
        || screen.3 <= 0
    {
        return false;
    }
    let right = state.x.saturating_add(state.width);
    let bottom = state.y.saturating_add(state.height);
    let screen_right = screen.0.saturating_add(screen.2);
    let screen_bottom = screen.1.saturating_add(screen.3);
    right
        .min(screen_right)
        .saturating_sub(state.x.max(screen.0))
        >= MIN_VISIBLE_EDGE
        && bottom
            .min(screen_bottom)
            .saturating_sub(state.y.max(screen.1))
            >= MIN_VISIBLE_EDGE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(x: i32, y: i32, width: i32, height: i32) -> MainWindowState {
        MainWindowState {
            schema_version: SCHEMA_VERSION,
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn saved_geometry_is_the_client_area_in_points() {
        // A 125% display with a 16x39px frame: the v1 code stored 1516x839
        // physical outer pixels and reopened that as logical points, so the
        // window grew by the frame *and* the scale factor on every launch.
        let scale = 1.25;
        assert_eq!(to_points(1516 - 16, scale), 1200);
        assert_eq!(to_points(839 - 39, scale), 640);
        // At 100% the scale is a no-op but the frame is not.
        assert_eq!(to_points(1516 - 16, 1.0), 1500);
        // A bad DPI read must not shrink the window to nothing.
        assert_eq!(to_points(1200, 0.0), 1200);
    }

    #[test]
    fn geometry_requires_a_visible_edge_and_bounded_size() {
        // Sized from the window minimums so raising one cannot silently turn
        // these cases into size-guard rejections and stop testing geometry.
        let w = crate::MIN_WINDOW_WIDTH as i32 + 300;
        let h = crate::MIN_WINDOW_HEIGHT as i32;
        let screen = (-1920, 0, 3840, 1080);

        // Mostly off the left of a dual-monitor desktop, but still reachable.
        assert!(is_visible(state(-1600, 100, w, h), screen));
        // Entirely past the right edge, and just past it.
        assert!(!is_visible(state(4000, 100, w, h), screen));
        assert!(!is_visible(state(3800, 100, w, h), screen));
        // Below the window minimums.
        assert!(!is_visible(state(0, 0, 10, 10), screen));
        assert!(!is_visible(
            state(0, 0, w, crate::MIN_WINDOW_HEIGHT as i32 - 1),
            screen
        ));
    }
}
