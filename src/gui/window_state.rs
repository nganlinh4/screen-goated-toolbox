use serde::{Deserialize, Serialize};
#[cfg(debug_assertions)]
pub(crate) mod acceptance;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetSystemMetrics, GetWindowPlacement, GetWindowRect, IsWindowVisible,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWMAXIMIZED,
    SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, ShowWindow,
    WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED,
};

/// Bumped to 2 when the stored size changed meaning: v1 wrote the *outer*
/// window rect in *physical pixels* but restored it through `with_inner_size`,
/// which reads *logical points*. Bumped to 3 when the position changed meaning:
/// v2 divided workspace pixels by the source monitor's scale and winit then
/// multiplied by the new window's scale, so a window on a monitor with another
/// scale, or beside a left or top taskbar, reopened somewhere else. The position
/// is now physical screen pixels, applied once the window exists. Old files are
/// discarded rather than reopening a window in the wrong place.
const SCHEMA_VERSION: u32 = 3;

/// Points per inch that Windows calls 100% scaling.
const DEFAULT_DPI: f32 = 96.0;
const MAX_STATE_BYTES: u64 = 4096;
const MIN_VISIBLE_EDGE: i32 = 64;

/// `x`/`y` are the outer window's top-left in physical screen pixels;
/// `width`/`height` are the client area in logical points.
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

/// The physical screen position [`apply_restored_position`] moves a restored,
/// unmaximized window to.
static RESTORED_POSITION: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);
static NORMAL_FRAME_POINTS: std::sync::Mutex<(f32, f32)> = std::sync::Mutex::new((0.0, 0.0));
static RESTORED_MAXIMIZED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static CHROME_RESTORE_COMPLETE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

pub(crate) fn available_size_points() -> (f32, f32) {
    let (point, area) = crate::overlay::display_placement::cursor_work_area();
    let scale = crate::overlay::display_placement::scale_at_point(point) as f32;
    (
        (area.width as f32 / scale - 24.0).max(1.0),
        (area.height as f32 / scale - 56.0).max(1.0),
    )
}

pub(crate) fn available_size_points_for_window(
    hwnd: windows::Win32::Foundation::HWND,
) -> (f32, f32) {
    let area = crate::overlay::display_placement::window_work_area(hwnd);
    let scale = window_scale(hwnd);
    (
        (area.width as f32 / scale - 24.0).max(1.0),
        (area.height as f32 / scale - 56.0).max(1.0),
    )
}

/// Applies the saved size and maximized state to the new window. Its position
/// is physical and is applied by [`apply_restored_position`] once the window
/// exists: a logical position would be scaled by whichever monitor the window
/// happens to be created on.
pub(crate) fn restore(viewport: eframe::egui::ViewportBuilder) -> eframe::egui::ViewportBuilder {
    let Some(state) = load().filter(|state| {
        let origin = POINT {
            x: state.x,
            y: state.y,
        };
        let scale = crate::overlay::display_placement::scale_at_point(origin) as f32;
        is_visible(*state, scale, virtual_screen())
    }) else {
        return viewport;
    };
    RESTORED_GEOMETRY.store(true, std::sync::atomic::Ordering::Relaxed);
    RESTORED_MAXIMIZED.store(state.maximized, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut size) = RESTORED_SIZE.lock() {
        *size = Some((state.width as f32, state.height as f32));
    }
    if let Ok(mut position) = RESTORED_POSITION.lock() {
        *position = restored_position(state);
    }
    viewport
        .with_inner_size([state.width as f32, state.height as f32])
        // Establish the normal rectangle on its saved monitor before maximizing.
        .with_maximized(false)
}

fn restored_position(state: MainWindowState) -> Option<(i32, i32)> {
    Some((state.x, state.y))
}

pub(crate) fn complete_chrome_restore() {
    CHROME_RESTORE_COMPLETE.store(true, std::sync::atomic::Ordering::Release);
    apply_restored_maximized();
}

pub(crate) fn apply_restored_maximized() {
    use std::sync::atomic::Ordering;
    if !CHROME_RESTORE_COMPLETE.load(Ordering::Acquire) {
        return;
    }
    let hwnd = unsafe { crate::gui::app::main_window_hwnd() };
    if hwnd.is_invalid() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return;
    }
    if RESTORED_MAXIMIZED.swap(false, Ordering::AcqRel) {
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWMAXIMIZED) };
    }
}

/// Moves the main window to its saved screen position, in physical pixels.
/// Landing on a monitor with another scale makes Windows rescale the window to
/// the same logical size there.
pub(crate) fn apply_restored_position() {
    let Some((x, y)) = RESTORED_POSITION
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
    else {
        return;
    };
    let hwnd = unsafe { crate::gui::app::main_window_hwnd() };
    if hwnd.is_invalid() {
        return;
    }
    let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, flags) };
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
    let mut outer = RECT::default();
    let shown_normal = placement.showCmd == SW_SHOWNORMAL.0 as u32
        && unsafe { GetWindowRect(hwnd, &mut outer) }.is_ok();
    let (frame_width, frame_height) = normal_placement_frame(
        window_frame_size(hwnd),
        scale,
        shown_normal,
        &mut NORMAL_FRAME_POINTS
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    );
    let (x, y) = screen_origin(rect, shown_normal.then_some(outer), workspace_offset(rect));
    let state = MainWindowState {
        schema_version: SCHEMA_VERSION,
        x,
        y,
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
    if !is_visible(state, scale, virtual_screen()) {
        return;
    }
    let path = state_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        let _ = crate::atomic_json::write_json_atomic(&path, &state);
    }
}

fn normal_placement_frame(
    observed: (i32, i32),
    scale: f32,
    shown_normal: bool,
    normal_points: &mut (f32, f32),
) -> (i32, i32) {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    if shown_normal {
        *normal_points = (observed.0 as f32 / scale, observed.1 as f32 / scale);
        observed
    } else {
        // A maximized window's resize borders are not its normal non-client frame.
        (
            (normal_points.0 * scale).round() as i32,
            (normal_points.1 * scale).round() as i32,
        )
    }
}

/// The restored window's top-left in screen pixels. A normally shown window
/// reports it directly; otherwise `rcNormalPosition` is in workspace
/// coordinates, which are offset by any taskbar on the left or top.
fn screen_origin(normal: RECT, shown_normal: Option<RECT>, offset: (i32, i32)) -> (i32, i32) {
    match shown_normal {
        Some(outer) => (outer.left, outer.top),
        None => (
            normal.left.saturating_add(offset.0),
            normal.top.saturating_add(offset.1),
        ),
    }
}

/// How far the work area of the monitor holding `rect` is from that
/// monitor's origin: the shift from workspace to screen coordinates.
fn workspace_offset(rect: RECT) -> (i32, i32) {
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return (0, 0);
    }
    (
        info.rcWork.left - info.rcMonitor.left,
        info.rcWork.top - info.rcMonitor.top,
    )
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
    crate::paths::app_runtime_local_data_dir().join("main-window-state.json")
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

/// Whether enough of the window would be on screen to grab it. Position and
/// screen are physical pixels; the size is points at `scale`.
fn is_visible(state: MainWindowState, scale: f32, screen: (i32, i32, i32, i32)) -> bool {
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
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let right = state
        .x
        .saturating_add((state.width as f32 * scale).round() as i32);
    let bottom = state
        .y
        .saturating_add((state.height as f32 * scale).round() as i32);
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

    #[test]
    fn window_geometry_maximized_placement_keeps_normal_client_size() {
        let mut frame = (0.0, 0.0);
        assert_eq!(
            normal_placement_frame((0, 0), 1.0, true, &mut frame),
            (0, 0)
        );
        let maximized_frame = normal_placement_frame((16, 16), 1.0, false, &mut frame);
        let mut saved = state(791, 156, 1044 - maximized_frame.0, 720 - maximized_frame.1);
        saved.maximized = true;
        assert!(is_visible(saved, 1.0, (0, 0, 2560, 1080)));
        assert_eq!((saved.width, saved.height), (1044, 720));
        normal_placement_frame((16, 32), 1.0, true, &mut frame);
        assert_eq!(
            normal_placement_frame((40, 40), 1.5, false, &mut frame),
            (24, 48)
        );
    }

    #[test]
    fn window_geometry_maximized_restoration_keeps_saved_monitor_origin() {
        let mut saved = state(-1600, 100, 1000, 720);
        saved.maximized = true;
        assert_eq!(restored_position(saved), Some((-1600, 100)));
    }

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

    fn is_visible_at_100(state: MainWindowState, screen: (i32, i32, i32, i32)) -> bool {
        is_visible(state, 1.0, screen)
    }

    #[test]
    fn the_saved_position_is_physical_screen_pixels() {
        let rect = |left, top| RECT {
            left,
            top,
            right: left + 1500,
            bottom: top + 900,
        };
        // A window on a 150% monitor at x=2560 is saved at 2560, not 1707.
        assert_eq!(
            screen_origin(rect(0, 0), Some(rect(2560, 100)), (0, 0)),
            (2560, 100)
        );
        // Maximized or minimized, the workspace rect is shifted by a left or
        // top taskbar to land in screen pixels.
        assert_eq!(screen_origin(rect(2500, 100), None, (60, 0)), (2560, 100));
        assert_eq!(screen_origin(rect(100, 0), None, (0, 48)), (100, 48));
    }

    #[test]
    fn a_window_sized_in_points_is_measured_at_its_scale() {
        let w = crate::MIN_WINDOW_WIDTH as i32;
        let h = crate::MIN_WINDOW_HEIGHT as i32;
        let screen = (0, 0, 3840, 2160);
        // Hanging off the left edge: at 100% too little is left on screen,
        // at 200% the same point size reaches far enough in.
        let x = -w + 40;
        assert!(!is_visible(state(x, 100, w, h), 1.0, screen));
        assert!(is_visible(state(x, 100, w, h), 2.0, screen));
    }

    #[test]
    fn geometry_requires_a_visible_edge_and_bounded_size() {
        // Sized from the window minimums so raising one cannot silently turn
        // these cases into size-guard rejections and stop testing geometry.
        let w = crate::MIN_WINDOW_WIDTH as i32 + 300;
        let h = crate::MIN_WINDOW_HEIGHT as i32;
        let screen = (-1920, 0, 3840, 1080);

        // Mostly off the left of a dual-monitor desktop, but still reachable.
        assert!(is_visible_at_100(state(-1600, 100, w, h), screen));
        // Entirely past the right edge, and just past it.
        assert!(!is_visible_at_100(state(4000, 100, w, h), screen));
        assert!(!is_visible_at_100(state(3800, 100, w, h), screen));
        // Below the window minimums.
        assert!(!is_visible_at_100(state(0, 0, 10, 10), screen));
        assert!(!is_visible_at_100(
            state(0, 0, w, crate::MIN_WINDOW_HEIGHT as i32 - 1),
            screen
        ));
    }
}
