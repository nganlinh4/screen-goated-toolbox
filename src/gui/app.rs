mod hotkeys;
mod init;
pub mod input_handler;
mod logic;
mod rendering;
mod types;
mod utils;

pub use init::SettingsAppInit;
pub use types::SettingsApp;
pub(crate) use utils::accept_restore_activation;
pub(crate) use utils::main_window_hwnd;
pub use utils::{exit_app, request_open_downloaded_tools, restart_app, signal_restore_window};

use eframe::egui;

impl eframe::App for SettingsApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(visuals.panel_fill).to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // egui 0.34 hands the root viewport a `&mut Ui`; derive the Context for
        // the non-panel logic + overlays (which paint via Area / ctx painters).
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // Log first update
        static LOGGED_STARTUP: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !LOGGED_STARTUP.swap(true, std::sync::atomic::Ordering::SeqCst) {
            crate::log_info!("[Main] App Update Start - Main Thread Alive");
        }

        // During the native modal sizing loop, keep the synchronous Windows frame path limited
        // to responsive layout and paint. Polling, IPC, clipboard, tray and updater work can wait
        // until the first ordinary frame after the resize finishes.
        if !crate::gui::resize_subclass::is_live_resize() {
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
            self.update_startup(ctx);
            self.update_bubble_sync();
            self.update_splash(ctx);
            self.check_restore_signal(ctx);
            self.update_hotkey_recording(ctx);
            self.update_computer_control_hotkey_recording(ctx);
            self.update_screen_translate_hotkey_recording(ctx);
            self.handle_events(ctx);
            self.handle_close_request(ctx);
        }

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

            // Overlays
            self.render_fade_overlay(ctx);

            // Render Minimal Mode Overlay (Realtime)
            crate::overlay::realtime_egui::render_minimal_overlay(ctx);

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
    }

    fn on_exit(&mut self) {
        crate::gui::window_state::save_main_window();
        let _ = crate::overlay::three_d_generator::shutdown();
        crate::overlay::creation_runtime::shutdown();
        crate::overlay::screen_record::cleanup_on_app_exit();
        self.tray_icon = None;
    }
}
