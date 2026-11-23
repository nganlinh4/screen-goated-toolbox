mod config_merge;
mod config_save;
mod exit_preparation;
mod exit_requests;
mod exit_update;
mod hotkey_capture;
mod hotkeys;
mod init;
pub mod input_handler;
mod logic;
mod rendering;
mod settings_exit_gate;
mod startup;
mod types;
mod utils;

pub(crate) use exit_preparation::{RestartPrepareDenied, exit_prepared, prepare_to_exit};
pub(crate) use exit_requests::has_pending_exit_request;
pub(crate) use exit_requests::request_update_restart;
pub(crate) use exit_requests::reserve_automatic_restart;
pub use exit_requests::{exit_app, restart_app};
pub use init::SettingsAppInit;
pub(crate) use settings_exit_gate::{StartupPermit, begin_startup_operation};
pub use types::SettingsApp;
pub(crate) use utils::accept_restore_activation;
pub(crate) use utils::{main_window_hwnd, remember_main_window};
pub use utils::{request_open_downloaded_tools, signal_restore_window};

pub(crate) fn mark_secondary_file_activation() {
    settings_exit_gate::mark_file_activation();
}

pub(crate) fn automatic_restart_blocked_by_file() -> bool {
    settings_exit_gate::automatic_restart_blocked()
}

use eframe::egui;

impl eframe::App for SettingsApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(visuals.panel_fill).to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        utils::remember_main_window(frame);
        let Some(settings_frame) = settings_exit_gate::begin_frame() else {
            settings_exit_gate::cancel_close_during_exit(ui.ctx());
            return;
        };
        // egui 0.34 hands the root viewport a `&mut Ui`; derive the Context for
        // the non-panel logic + overlays (which paint via Area / ctx painters).
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        if self.config_conflict.is_none() {
            self.poll_startup(ctx);
        }
        if self.config_conflict.is_some() {
            self.check_restore_signal(ctx);
            logic::handle_close_request(ctx);
            self.render_config_save_failure(ui);
            if self.config_conflict.is_none() && self.startup.busy() {
                ctx.request_repaint();
            }
            settings_frame.finish(
                self.config_conflict.is_some(),
                self.pending_file_path.is_none()
                    && !self.startup.busy()
                    && !self.config_save.is_pending(),
            );
            exit_requests::dispatch(self.config_conflict.is_some(), ctx);
            return;
        }
        self.refresh_shared_settings(ctx);
        self.poll_screen_translate_region_edit(ctx);
        // Log first update
        static LOGGED_STARTUP: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !LOGGED_STARTUP.swap(true, std::sync::atomic::Ordering::SeqCst) {
            crate::log_info!("[Main] App Update Start - Main Thread Alive");
        }

        // During the native modal sizing loop, keep the synchronous Windows frame path limited
        // to responsive layout and paint. Polling, IPC, clipboard, tray and updater work can wait
        // until the first ordinary frame after the resize finishes.
        logic::poll_and_handle_close(ctx, crate::gui::resize_subclass::is_live_resize(), || {
            // Handle Dropped Files and Paste FIRST (before any UI consumes events)
            if let Some(path) = self.pending_file_path.take() {
                crate::log_info!("App Update: Found pending file path, triggering process...");
                input_handler::process_file_path(&path);
            }
            input_handler::handle_dropped_files(ctx);
            if !self.download_manager.show_window && !self.show_tts_playground {
                input_handler::handle_paste(ctx);
            }

            self.check_updater();
            self.update_theme_and_tray(ctx);
            self.pulse_custom_chrome_resize_if_pending(ctx);
            self.persist_geometry_when_settled(ctx);
            self.update_startup(ctx);
            self.update_bubble_sync();
            self.update_splash(ctx);
            self.check_restore_signal(ctx);
            self.update_hotkey_recording(ctx);
            self.update_computer_control_hotkey_recording(ctx);
            self.update_screen_translate_hotkey_recording(ctx);
            self.update_live_translate_hotkey_recording(ctx);
            self.handle_events(ctx);
        });

        let main_ui_ready = self.is_main_ui_ready();

        // --- RESIZE SUBCLASS (once, after window is visible) ---
        if main_ui_ready && !self.resize_subclass_installed {
            unsafe {
                let hwnd = utils::main_window_hwnd();
                if !hwnd.is_invalid() {
                    crate::gui::resize_subclass::install(hwnd);
                    self.resize_subclass_installed = true;
                }
            }
        }

        // --- UI LAYOUT ---
        if main_ui_ready {
            // Decide the responsive detail columns first — the title bar hides the
            // tabs for panels that are shown as columns this frame.
            self.update_detail_layout(ctx);

            // Title Bar (Custom Windows Bar) — top panel, shown into the root ui.
            if self.custom_chrome_ready {
                self.render_title_bar(ui);
            }

            // Footer & Tips Modal — bottom panel.
            self.render_footer_and_tips_modal(ui);

            let tts_playground_hovered = ctx
                .data(|data| data.get_temp::<bool>(egui::Id::new("tts_playground_hovered")))
                .unwrap_or(false);
            if tts_playground_hovered {
                ctx.input_mut(|input| {
                    input.smooth_scroll_delta = egui::Vec2::ZERO;
                });
            }

            // Main Layout — central panel, fills the remaining root ui.
            self.render_main_layout(ui);
            self.render_preset_model_update_modal(ui);
            if self.config_conflict.is_none() {
                exit_requests::render_failure(ui, &self.config.ui_language);
            }

            // Computer Control's UI is the transparent "orb" overlay (its own WebView window,
            // driven from the runtime thread) — nothing to render in egui here.
        }

        // Render Splash Overlay (Last Last)
        // Note: Splash remains visible during its exit animation, covering the UI.
        if let Some(splash) = &self.splash
            && splash.paint(ctx, &self.config.theme_mode)
        {
            let is_currently_dark = ctx.global_style().visuals.dark_mode;
            self.config.theme_mode = if is_currently_dark {
                crate::config::ThemeMode::Light
            } else {
                crate::config::ThemeMode::Dark
            };
            self.save_and_sync();
        }

        // Render Drop Overlay when dragging files (Very Last)
        if main_ui_ready {
            self.render_drop_overlay(ctx);
        }
        #[cfg(debug_assertions)]
        if main_ui_ready && self.splash.is_none() {
            crate::gui::window_state::acceptance::tick(ctx);
        }

        // Register the lightweight off-screen tray child while the root UI is
        // available. Tray interactions later address only this child HWND.
        crate::overlay::tray_popup::render(ctx);
        self.sync_hotkey_capture(ctx);
        self.flush_config_save_when_due(ctx);
        if let Some(conflict) = self.config_conflict.as_mut() {
            conflict.capture_frame_edits(&self.config);
            ctx.request_repaint();
        }
        settings_frame.finish(
            self.config_conflict.is_some(),
            self.pending_file_path.is_none()
                && !self.startup.busy()
                && !self.config_save.is_pending(),
        );
        exit_requests::dispatch(self.config_conflict.is_some(), ctx);
    }

    fn on_exit(&mut self) {
        self.flush_config_save_now();
        exit_preparation::release_owned_resources();
        if let Err(error) = crate::overlay::screen_record::cleanup_on_app_exit() {
            crate::log_info!(
                "[Shutdown] recorder preparation failed during window exit: {error:#}"
            );
        }
        self.tray_icon = None;
    }
}
