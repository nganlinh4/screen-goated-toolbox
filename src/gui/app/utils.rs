use super::types::{REQUEST_OPEN_DOWNLOADED_TOOLS, RESTORE_SIGNAL, SettingsApp};
use crate::config::save_config;
use eframe::egui;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, SW_RESTORE,
    SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};
use windows::core::*;
use windows_core::BOOL;

/// Public function to signal the main window to restore (called from tray popup)
pub fn signal_restore_window() {
    accept_restore_activation();
}

pub(crate) fn accept_restore_activation() {
    show_main_window_native();
    RESTORE_SIGNAL.store(true, Ordering::SeqCst);
    if let Ok(ctx) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = ctx.as_ref()
    {
        ctx.request_repaint();
    }
}

pub fn request_open_downloaded_tools() {
    REQUEST_OPEN_DOWNLOADED_TOOLS.store(true, Ordering::SeqCst);
    signal_restore_window();
    if let Ok(ctx) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = ctx.as_ref()
    {
        ctx.request_repaint();
    }
}

impl SettingsApp {
    pub(crate) fn ensure_custom_chrome(&mut self, ctx: &egui::Context) {
        if self.custom_chrome_ready {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        // The main window is visually opaque. Keeping a transparent swap chain here
        // exposes the desktop (and then the clear texture) for a frame whenever the
        // Win32 client rect outruns wgpu during a fast live resize. DWM supplies the
        // rounded outer corners, so per-pixel transparency is unnecessary.
        ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(false));

        // Dropping the decorations keeps the *outer* rect and hands the vacated
        // caption and borders to the client area, so the window silently grew by
        // the frame — about 39pt taller and 16pt wider — on every launch. Ask for
        // the client size we actually wanted, now that the frame is gone.
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        if !maximized {
            let (width, height) = crate::gui::window_state::restored_inner_size()
                .unwrap_or((crate::WINDOW_WIDTH, crate::WINDOW_HEIGHT));
            crate::log_info!(
                "[WindowState] re-applying client size after chrome: {width}x{height}"
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
        }

        self.custom_chrome_ready = true;
        // Let that resize land before the repaint pulse samples the window, or the
        // pulse restores the pre-resize rect and undoes it.
        self.custom_chrome_resize_pulse_stage = 4;
        self.custom_chrome_restore_size = None;
        ctx.request_repaint();
    }

    pub(crate) fn pulse_custom_chrome_resize_if_pending(&mut self, ctx: &egui::Context) {
        match self.custom_chrome_resize_pulse_stage {
            0 => return,
            // One frame of grace for the post-decoration resize.
            4 => {
                self.custom_chrome_resize_pulse_stage = 1;
            }
            1 => {
                self.custom_chrome_restore_size = begin_main_window_resize_pulse();
                self.custom_chrome_resize_pulse_stage = 2;
            }
            2 => {
                if let Some((width, height)) = self.custom_chrome_restore_size {
                    restore_main_window_resize_pulse(width, height);
                } else {
                    force_main_window_frame_changed();
                }
                self.custom_chrome_resize_pulse_stage = 3;
            }
            _ => {
                force_main_window_frame_changed();
                self.custom_chrome_resize_pulse_stage = 0;
                self.custom_chrome_restore_size = None;
            }
        }
        ctx.request_repaint();
    }

    /// Writes the window geometry once the user stops dragging it.
    ///
    /// This is the app's only continuous save: exit is not a reliable moment,
    /// because the close button hides to tray, tray Quit calls `process::exit`,
    /// and a development run is usually killed outright — none of which unwind.
    /// Saving when the geometry settles means a resize, move, or maximize
    /// survives however the process ends, including a kill.
    pub(crate) fn persist_geometry_when_settled(&mut self, ctx: &egui::Context) {
        if !self.is_main_ui_ready() {
            return;
        }

        let geometry = ctx.input(|input| {
            let viewport = input.viewport();
            viewport.outer_rect.map(|outer| {
                let size = viewport
                    .inner_rect
                    .map_or(outer.size(), |inner| inner.size());
                (outer.min.x, outer.min.y, size.x, size.y)
            })
        });
        let Some(geometry) = geometry else {
            return;
        };

        let moved = self
            .last_window_geometry
            .is_none_or(|last| geometry_differs(last, geometry));
        if moved {
            self.last_window_geometry = Some(geometry);
            self.window_geometry_changed_at = Some(std::time::Instant::now());
            return;
        }

        let Some(changed_at) = self.window_geometry_changed_at else {
            return;
        };
        if crate::gui::resize_subclass::is_live_resize()
            || changed_at.elapsed() < GEOMETRY_SETTLE_DELAY
        {
            return;
        }

        self.window_geometry_changed_at = None;
        crate::gui::window_state::save_main_window();
    }

    pub(crate) fn is_main_ui_ready(&self) -> bool {
        self.startup_stage >= 38
            && self.custom_chrome_ready
            && self.custom_chrome_resize_pulse_stage == 0
    }

    /// Pull app-level hotkeys that may be edited outside the main egui window
    /// before checking conflicts or saving a new binding.
    pub(crate) fn sync_global_hotkeys(&mut self) {
        if let Ok(state) = self.app_state_ref.lock() {
            self.config.screen_record_hotkeys = state.config.screen_record_hotkeys.clone();
            self.config.computer_control_hotkeys = state.config.computer_control_hotkeys.clone();
            self.config.screen_translate.hotkeys = state.config.screen_translate.hotkeys.clone();
            self.config.translation_gummy.hotkeys = state.config.translation_gummy.hotkeys.clone();
        }
    }

    pub(crate) fn save_and_sync(&mut self) {
        if let crate::gui::settings_ui::ViewMode::Preset(idx) = self.view_mode {
            self.config.active_preset_idx = idx;
        }
        self.config.sync_active_profile_from_presets();

        let mut state = self.app_state_ref.lock().unwrap();

        // Pull fields that can be modified by external modules (tray popup, bubble panel)
        // before overwriting the global state, to avoid clobbering their changes.
        self.config.show_favorite_bubble = state.config.show_favorite_bubble;
        self.config.favorite_bubble_position = state.config.favorite_bubble_position;
        self.config.favorite_bubble_size = state.config.favorite_bubble_size;
        self.config.favorites_keep_open = state.config.favorites_keep_open;
        state.hotkeys_updated = true;
        state.config = self.config.clone();
        drop(state);
        save_config(&self.config);

        // Sync PromptDJ and ScreenRecord settings if windows are active
        crate::overlay::prompt_dj::update_settings();
        crate::overlay::translation_gummy::update_settings();
        crate::overlay::screen_record::update_settings();
        crate::overlay::three_d_generator::update_settings();
        crate::overlay::image_to_svg::update_settings();
        crate::overlay::image_creator::update_settings();

        unsafe {
            let class = w!("HotkeyListenerClass");
            let title = w!("Listener");
            let hwnd = windows::Win32::UI::WindowsAndMessaging::FindWindowW(class, title)
                .unwrap_or_default();
            if !hwnd.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    0x0400 + 101,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    pub(crate) fn restore_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::Normal,
        ));
        show_main_window_native();
        self.ensure_custom_chrome(ctx);
        ctx.request_repaint();
    }

    pub(crate) fn check_hotkey_conflict(
        &self,
        vk: u32,
        mods: u32,
        current_preset_idx: usize,
    ) -> Option<crate::config::HotkeyConflict> {
        self.config
            .check_hotkey_conflict(vk, mods, Some(current_preset_idx))
    }
}

pub(crate) fn show_main_window_native() {
    unsafe {
        let hwnd = main_window_hwnd();
        if !hwnd.is_invalid() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
    }
}

fn force_main_window_frame_changed() {
    unsafe {
        let hwnd = main_window_hwnd();
        if hwnd.is_invalid() {
            return;
        }

        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// How long the window must hold still before its geometry is written.
const GEOMETRY_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

/// Whether two geometries differ by enough to be a real move, not jitter.
fn geometry_differs(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    (a.0 - b.0).abs() > 0.5
        || (a.1 - b.1).abs() > 0.5
        || (a.2 - b.2).abs() > 0.5
        || (a.3 - b.3).abs() > 0.5
}

fn begin_main_window_resize_pulse() -> Option<(i32, i32)> {
    unsafe {
        let hwnd = main_window_hwnd();
        if hwnd.is_invalid() {
            return None;
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            force_main_window_frame_changed();
            return None;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            force_main_window_frame_changed();
            return None;
        }

        crate::log_info!(
            "[Startup] Pulsing main window resize after custom chrome: {}x{} -> {}x{}",
            width,
            height,
            width + 1,
            height
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width + 1,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        Some((width, height))
    }
}

fn restore_main_window_resize_pulse(width: i32, height: i32) {
    unsafe {
        let hwnd = main_window_hwnd();
        if hwnd.is_invalid() {
            return;
        }

        crate::log_info!(
            "[Startup] Restoring main window size after custom chrome pulse: {}x{}",
            width,
            height
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Title of the eframe main window.
pub(crate) const MAIN_WINDOW_TITLE: PCWSTR = w!("Screen Goated Toolbox (SGT by nganlinh4)");

struct MainWindowSearch {
    process_id: u32,
    hwnd: windows::Win32::Foundation::HWND,
}

extern "system" fn find_process_eframe_window(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: LPARAM,
) -> BOOL {
    unsafe {
        let search = &mut *(lparam.0 as *mut MainWindowSearch);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id != search.process_id {
            return BOOL(1);
        }

        const EFRAME_CLASS: &[u16] = &[
            b'e' as u16,
            b'f' as u16,
            b'r' as u16,
            b'a' as u16,
            b'm' as u16,
            b'e' as u16,
        ];
        let mut class_name = [0u16; 32];
        let length = GetClassNameW(hwnd, &mut class_name);
        if length > 0 && &class_name[..length as usize] == EFRAME_CLASS {
            search.hwnd = hwnd;
            return BOOL(0);
        }
        BOOL(1)
    }
}

/// Locate this process's eframe window without selecting a normal/smoke sibling.
pub(crate) unsafe fn main_window_hwnd() -> windows::Win32::Foundation::HWND {
    let mut search = MainWindowSearch {
        process_id: std::process::id(),
        hwnd: windows::Win32::Foundation::HWND::default(),
    };
    unsafe {
        let _ = EnumWindows(
            Some(find_process_eframe_window),
            LPARAM(&mut search as *mut _ as isize),
        );
    }
    if !search.hwnd.is_invalid() {
        return search.hwnd;
    }

    // Preserve the title fallback, but never return another process's window.
    let hwnd = unsafe { FindWindowW(None, MAIN_WINDOW_TITLE).unwrap_or_default() };
    let mut process_id = 0;
    if !hwnd.is_invalid() {
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    }
    if process_id == std::process::id() {
        hwnd
    } else {
        windows::Win32::Foundation::HWND::default()
    }
}

/// Robustly restart the application on Windows.
/// Uses a temporary batch file with a small delay to ensure the current process exits
/// and releases its single-instance mutex before the new instance starts.
pub fn restart_app() {
    if let Ok(exe_path) = std::env::current_exe() {
        // Create a temporary batch file to handle the delayed restart reliably
        let kill_mutex_cmd = "timeout /t 1 /nobreak > NUL".to_string();
        // Pass --restarted flag to show notification on next start
        let start_cmd = format!("start \"\" \"{}\" --restarted", exe_path.to_string_lossy());
        let self_del_cmd = "(goto) 2>nul & del \"%~f0\"";

        let batch_content = format!(
            "@echo off\r\n{}\r\n{}\r\n{}",
            kill_mutex_cmd, start_cmd, self_del_cmd
        );

        let temp_dir = std::env::temp_dir();
        let bat_path = temp_dir.join(format!("sgt_restart_{}.bat", std::process::id()));

        if std::fs::write(&bat_path, batch_content).is_ok() {
            // Spawn the batch file hidden via cmd /C with CREATE_NO_WINDOW
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd")
                .args(["/C", &bat_path.to_string_lossy()])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn();
            exit_app();
        } else {
            // Fallback: Just try to spawn directly if batch fails
            let _ = std::process::Command::new(exe_path)
                .arg("--restarted")
                .spawn();
            exit_app();
        }
    }
}

/// Cleanly exit the process after best-effort recorder cleanup.
///
/// Flushes the window geometry first. `persist_geometry_when_settled` has
/// normally written it already; this catches a quit within the settle delay of
/// a resize. It cannot live in `App::on_exit` — `process::exit` never unwinds,
/// so eframe never sees the shutdown.
pub fn exit_app() -> ! {
    crate::gui::window_state::save_main_window();
    crate::overlay::screen_record::cleanup_on_app_exit();
    std::process::exit(0)
}
