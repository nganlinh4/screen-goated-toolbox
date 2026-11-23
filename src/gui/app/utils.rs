use super::types::{REQUEST_OPEN_DOWNLOADED_TOOLS, RESTORE_SIGNAL, SettingsApp};
use eframe::egui;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, SHOW_WINDOW_CMD, SW_RESTORE, SW_SHOW, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos,
    ShowWindow,
};

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
                crate::gui::window_state::complete_chrome_restore();
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
            ctx.request_repaint_after(GEOMETRY_SETTLE_DELAY);
            return;
        }

        let Some(changed_at) = self.window_geometry_changed_at else {
            return;
        };
        if !geometry_save_ready(
            ctx,
            changed_at.elapsed(),
            crate::gui::resize_subclass::is_live_resize(),
        ) {
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
            self.config.screen_translate.fullscreen_hotkeys =
                state.config.screen_translate.fullscreen_hotkeys.clone();
            self.config.screen_translate.subtitle_hotkeys =
                state.config.screen_translate.subtitle_hotkeys.clone();
            self.config.live_translate.hotkeys = state.config.live_translate.hotkeys.clone();
            self.config.translation_gummy.hotkeys = state.config.translation_gummy.hotkeys.clone();
        }
    }

    pub(crate) fn save_and_sync(&mut self) {
        if self.config_conflict.is_some() {
            return;
        }
        if let crate::gui::settings_ui::ViewMode::Preset(idx) = self.view_mode {
            self.config.active_preset_idx = idx;
        }
        self.config.sync_active_profile_from_presets();

        let realtime_active = crate::overlay::is_realtime_overlay_active();
        let mut state = self.app_state_ref.lock().unwrap();
        let registered = crate::hotkey::RegisteredBindings::of(&state.config);

        if realtime_active {
            self.config
                .sync_live_translate_overlay_controls_from(&state.config);
        }
        let merged = match super::config_merge::merge_settings_edits(
            &self.config_baseline,
            &self.config,
            &state.config,
        ) {
            Ok(merged) => merged,
            Err(error) => {
                crate::log_info!("[config] failed to merge settings edits: {error}");
                self.config_conflict = Some(super::config_merge::ConfigConflict {
                    base: self.config_baseline.clone(),
                    edited: self.config.clone(),
                });
                return;
            }
        };
        if let Err(error) = self
            .config_save
            .absorb_shared_changes(&self.config_baseline, &state.config)
        {
            crate::log_info!("[config] could not rebase pending save: {error}");
            return;
        }
        // The disk write is coalesced: a slider drag or typing commits every
        // frame, and one write follows once the edits go idle.
        self.config_save
            .record_edit(&state.config, std::time::Instant::now());
        state.config = merged.clone();
        // Slider drags and typing commit every frame; only a binding change needs
        // the listener to unregister and re-register everything.
        let bindings_changed = crate::hotkey::RegisteredBindings::of(&merged) != registered;
        if bindings_changed {
            state.hotkeys_updated = true;
        }
        self.config = merged.clone();
        self.config_baseline = merged;
        drop(state);

        // Sync PromptDJ and ScreenRecord settings if windows are active
        crate::overlay::prompt_dj::update_settings();
        crate::overlay::translation_gummy::update_settings();
        crate::overlay::screen_record::update_settings();
        crate::overlay::three_d_generator::update_settings();
        crate::overlay::image_to_svg::update_settings();

        if bindings_changed {
            crate::hotkey::reload_registrations();
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
            let _ = ShowWindow(hwnd, main_window_show_command(IsIconic(hwnd).as_bool()));
            let _ = ShowWindow(hwnd, SW_SHOW);
            crate::gui::window_state::apply_restored_maximized();
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
    }
}

fn main_window_show_command(minimized: bool) -> SHOW_WINDOW_CMD {
    if minimized { SW_RESTORE } else { SW_SHOW }
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

fn geometry_save_ready(
    ctx: &egui::Context,
    elapsed: std::time::Duration,
    live_resize: bool,
) -> bool {
    if live_resize {
        return false;
    }
    if let Some(remaining) = GEOMETRY_SETTLE_DELAY.checked_sub(elapsed)
        && !remaining.is_zero()
    {
        ctx.request_repaint_after(remaining);
        return false;
    }
    true
}

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

/// The main window, captured from eframe's own window handle when the app is
/// created; zero until then.
static MAIN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Records the main window from eframe's raw handle. Called with the creation
/// context before anything asks for the window, and again per frame as a
/// no-op; later lookups only read an atomic.
pub(crate) fn remember_main_window(window: &impl raw_window_handle::HasWindowHandle) {
    remember_window(&MAIN_HWND, window);
}

/// This process's main window, or a null handle before it has been created.
pub(crate) unsafe fn main_window_hwnd() -> windows::Win32::Foundation::HWND {
    remembered_window(&MAIN_HWND)
}

fn remember_window(
    slot: &std::sync::atomic::AtomicIsize,
    window: &impl raw_window_handle::HasWindowHandle,
) {
    use raw_window_handle::RawWindowHandle;
    if slot.load(Ordering::Acquire) != 0 {
        return;
    }
    if let Ok(handle) = window.window_handle()
        && let RawWindowHandle::Win32(window) = handle.as_raw()
    {
        slot.store(window.hwnd.get(), Ordering::Release);
    }
}

fn remembered_window(slot: &std::sync::atomic::AtomicIsize) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(slot.load(Ordering::Acquire) as *mut std::ffi::c_void)
}

#[cfg(test)]
mod main_window_tests {
    use super::{remember_window, remembered_window};
    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
    };
    use std::num::NonZeroIsize;
    use std::sync::atomic::AtomicIsize;

    #[test]
    fn window_geometry_show_preserves_maximized_state() {
        assert_eq!(super::main_window_show_command(false), super::SW_SHOW);
        assert_eq!(super::main_window_show_command(true), super::SW_RESTORE);
    }

    #[test]
    fn idle_geometry_save_schedules_its_own_wake() {
        let ctx = eframe::egui::Context::default();
        for _ in 0..4 {
            let mut output = ctx.run_ui(Default::default(), |_| {});
            output.textures_delta.clear();
        }
        let mut output = ctx.run_ui(Default::default(), |ui| {
            assert!(!super::geometry_save_ready(
                ui.ctx(),
                std::time::Duration::from_millis(125),
                false,
            ));
        });
        let delay = output.viewport_output[&eframe::egui::ViewportId::ROOT].repaint_delay;
        output.textures_delta.clear();
        assert!(
            !delay.is_zero() && delay <= std::time::Duration::from_millis(275),
            "an idle window must wake to save without another user event",
        );
        assert!(super::geometry_save_ready(
            &ctx,
            super::GEOMETRY_SETTLE_DELAY,
            false,
        ));
        assert!(!super::geometry_save_ready(
            &ctx,
            super::GEOMETRY_SETTLE_DELAY,
            true,
        ));
    }

    struct Window(isize);
    impl HasWindowHandle for Window {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw =
                RawWindowHandle::Win32(Win32WindowHandle::new(NonZeroIsize::new(self.0).unwrap()));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    #[test]
    fn the_window_eframe_created_is_the_only_one_ever_returned() {
        let slot = AtomicIsize::new(0);
        assert!(remembered_window(&slot).is_invalid());
        remember_window(&slot, &Window(0x1234));
        // The per-frame call cannot replace the window recorded at creation.
        remember_window(&slot, &Window(0x5678));
        assert_eq!(remembered_window(&slot).0 as isize, 0x1234);
    }
}
