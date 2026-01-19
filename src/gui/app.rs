mod init;
pub mod input_handler;
mod logic;
mod rendering;
mod types;
mod utils;

pub use types::SettingsApp;
pub use utils::{restart_app, signal_restore_window};

use eframe::egui;

impl eframe::App for SettingsApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Log first update
        static LOGGED_STARTUP: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !LOGGED_STARTUP.swap(true, std::sync::atomic::Ordering::SeqCst) {
            crate::log_info!("[Main] App Update Start - Main Thread Alive");
        }

        // Handle Dropped Files and Paste FIRST (before any UI consumes events)
        if let Some(path) = self.pending_file_path.take() {
            crate::log_info!("App Update: Found pending file path, triggering process...");
            input_handler::process_file_path(&path);
        }
        input_handler::handle_dropped_files(ctx);
        if !self.download_manager.show_window {
            input_handler::handle_paste(ctx);
        }

        // Updater
        self.check_updater();

        // Theme & Tray
        self.update_theme_and_tray(ctx);

        // Startup Logic
        self.update_startup(ctx);

        // Bubble Sync
        self.update_bubble_sync();

        // Splash
        self.update_splash(ctx);

        // Restore Signal
        self.check_restore_signal(ctx);

        // Hotkey Recording
        self.update_hotkey_recording(ctx);

        // Event Handling
        self.handle_events(ctx);

        // Close Request
        self.handle_close_request(ctx);

        // Tips Logic
        self.update_tips_logic(ctx);

        // --- UI LAYOUT ---
        // Title Bar (Custom Windows Bar)
        self.render_title_bar(ctx);

        // Footer & Tips Modal
        self.render_footer_and_tips_modal(ctx);

        // Main Layout
        self.render_main_layout(ctx);

        // Window Resizing (Must be last to override cursors at edges)
        self.render_window_resize_handles(ctx);

        // Overlays
        self.render_fade_overlay(ctx);
        self.render_drop_overlay(ctx);

        // Render Minimal Mode Overlay (Realtime)
        crate::overlay::realtime_egui::render_minimal_overlay(ctx);

        // Render Splash Overlay (Last Last)
        if let Some(splash) = &self.splash {
            if splash.paint(ctx, &self.config.theme_mode) {
                // Simplified Toggle: Toggle between Light and Dark only
                // If current effective is Dark, switch to Light, and vice versa.
                let is_currently_dark = ctx.style().visuals.dark_mode;
                self.config.theme_mode = if is_currently_dark {
                    crate::config::ThemeMode::Light
                } else {
                    crate::config::ThemeMode::Dark
                };
                self.save_and_sync();
            }
        }

        // Render Drop Overlay when dragging files (Very Last)
        self.render_drop_overlay(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.tray_icon = None;
    }
}
