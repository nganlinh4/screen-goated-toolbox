use serde::{Deserialize, Serialize};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowPlacement, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWMAXIMIZED, WINDOWPLACEMENT,
    WPF_RESTORETOMAXIMIZED,
};

const SCHEMA_VERSION: u32 = 1;
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

pub(crate) fn restore(viewport: eframe::egui::ViewportBuilder) -> eframe::egui::ViewportBuilder {
    let Some(state) = load().filter(|state| is_visible(*state, virtual_screen())) else {
        return viewport;
    };
    viewport
        .with_position([state.x as f32, state.y as f32])
        .with_inner_size([state.width as f32, state.height as f32])
        .with_maximized(state.maximized)
}

pub(super) fn save_main_window() {
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
    let rect = placement.rcNormalPosition;
    let state = MainWindowState {
        schema_version: SCHEMA_VERSION,
        x: rect.left,
        y: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
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
    fn geometry_requires_a_visible_edge_and_bounded_size() {
        let screen = (-1920, 0, 3840, 1080);
        assert!(is_visible(state(-1600, 100, 1400, 700), screen));
        assert!(!is_visible(state(4000, 100, 1400, 700), screen));
        assert!(!is_visible(state(1900, 100, 1400, 700), screen));
        assert!(!is_visible(state(0, 0, 10, 10), screen));
    }
}
