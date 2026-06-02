use super::types::{RESTORE_SIGNAL, SettingsApp, UserEvent};
use crate::config::ThemeMode;
use crate::gui::app::utils::simple_rand;
use crate::gui::locale::LocaleText;
use crate::icon_gen;
use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

impl SettingsApp {
    pub(crate) fn check_updater(&mut self) {
        while let Ok(status) = self.update_rx.try_recv() {
            // Show popup notification when update is available
            if let crate::updater::UpdateStatus::UpdateAvailable { ref version, .. } = status {
                // Show blue-themed update notification with longer duration
                let ui_lang = self.config.ui_language.clone();
                let locale = crate::gui::locale::LocaleText::get(&ui_lang);
                let notification_text =
                    format!("{} v{}", locale.update_available_notification, version);
                crate::overlay::auto_copy_badge::show_update_notification(&notification_text);
            }
            self.update_status = status;
        }
    }

    pub(crate) fn update_theme_and_tray(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);

        // 1. Check if we need to poll system theme (only if in System mode)
        let mut current_system_dark = self.last_system_theme_dark;

        if now - self.theme_check_timer > 1.0 {
            self.theme_check_timer = now;
            // Always update system state tracker, even if not currently used
            current_system_dark = crate::gui::utils::is_system_in_dark_mode();
            self.last_system_theme_dark = current_system_dark;
        }

        // 2. Calculate Effective Theme
        let effective_dark = match self.config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => current_system_dark,
        };

        // 3. Apply Changes if Effective Theme Changed
        if effective_dark != self.last_effective_theme_dark {
            self.last_effective_theme_dark = effective_dark;

            // A. Update Visuals (egui)
            if effective_dark {
                ctx.set_visuals(egui::Visuals::dark());
            } else {
                ctx.set_visuals(egui::Visuals::light());
            }

            // B. Update Native Icons (Tray & Window) based on Effective Theme
            if let Some(tray) = &mut self.tray_icon {
                let new_icon = icon_gen::get_tray_icon(effective_dark);
                let _ = tray.set_icon(Some(new_icon));
            }
            crate::gui::utils::update_window_icon_native(effective_dark);

            // C. Update Realtime Webviews
            unsafe {
                use crate::api::realtime_audio::WM_THEME_UPDATE;
                use crate::overlay::realtime_webview::state::{REALTIME_HWND, TRANSLATION_HWND};
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

                let realtime_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
                if !realtime_hwnd.is_invalid() {
                    let _ =
                        PostMessageW(Some(realtime_hwnd), WM_THEME_UPDATE, WPARAM(0), LPARAM(0));
                }
                let translation_hwnd = std::ptr::addr_of!(TRANSLATION_HWND).read();
                if !translation_hwnd.is_invalid() {
                    let _ = PostMessageW(
                        Some(translation_hwnd),
                        WM_THEME_UPDATE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }

                crate::overlay::window_selector::update_theme(crate::overlay::is_dark_mode());
            }

            // D. Update Screen Record WebView
            crate::overlay::screen_record::update_settings();
            crate::overlay::translation_gummy::update_settings();
            crate::overlay::tts_playground::update_settings();

            // E. Update Favorite Bubble + Panel
            unsafe {
                use crate::overlay::favorite_bubble::{WM_BUBBLE_THEME_UPDATE, state::BUBBLE_HWND};
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

                let bubble_val = BUBBLE_HWND.load(std::sync::atomic::Ordering::SeqCst);
                if bubble_val != 0 {
                    let hwnd =
                        windows::Win32::Foundation::HWND(bubble_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), WM_BUBBLE_THEME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
        }

        // --- TRAY MENU I18N UPDATE ---
        // Update tray menu items when language changes
        if self.config.ui_language != self.last_ui_language {
            self.last_ui_language = self.config.ui_language.clone();
            let new_locale = LocaleText::get(&self.config.ui_language);
            self.tray_settings_item.set_text(new_locale.tray_settings);
            self.tray_quit_item.set_text(new_locale.tray_quit);
        }

        // --- LAZY TRAY ICON RECONCILE ---
        if self.tray_icon.is_none() && now - self.tray_retry_timer > 1.0 {
            self.tray_retry_timer = now;
            let icon = icon_gen::get_tray_icon(self.last_effective_theme_dark);
            if let Ok(tray) = TrayIconBuilder::new()
                .with_tooltip("Screen Goated Toolbox (nganlinh4)")
                .with_icon(icon)
                .build()
            {
                self.tray_icon = Some(tray);
            }
        }
    }

    pub(crate) fn update_startup(&mut self, ctx: &egui::Context) {
        const TRAY_STARTUP_GRACE_FRAMES: u16 = 180;

        if self.startup_stage == 0 {
            unsafe {
                let mut cursor_pos = POINT::default();
                let _ = GetCursorPos(&mut cursor_pos);
                let h_monitor = MonitorFromPoint(cursor_pos, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                let _ = GetMonitorInfoW(h_monitor, &mut mi);

                let work_w = (mi.rcWork.right - mi.rcWork.left) as f32;
                let work_h = (mi.rcWork.bottom - mi.rcWork.top) as f32;
                let work_left = mi.rcWork.left as f32;
                let work_top = mi.rcWork.top as f32;

                let pixels_per_point = ctx.pixels_per_point();
                let win_w_physical = WINDOW_WIDTH * pixels_per_point;
                let win_h_physical = WINDOW_HEIGHT * pixels_per_point;

                let center_x_physical = work_left + (work_w - win_w_physical) / 2.0;
                let center_y_physical = work_top + (work_h - win_h_physical) / 2.0;

                let x_logical = center_x_physical / pixels_per_point;
                let y_logical = center_y_physical / pixels_per_point;

                if !self.config.start_in_tray {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        x_logical, y_logical,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        WINDOW_WIDTH,
                        WINDOW_HEIGHT,
                    )));
                }

                self.startup_stage = 1;
                ctx.request_repaint();
            }
        } else if self.startup_stage == 1 {
            // --- EARLY INIT: TRULY BEFORE SPLASH ---

            // 1. Start favorite bubble (WebView creation)
            if self.config.show_favorite_bubble {
                crate::overlay::favorite_bubble::show_favorite_bubble();
            }

            // 2. Trigger auto-update check (Network/Disk IO)
            if let Some(updater) = &self.updater {
                updater.check_for_updates();
            }

            self.startup_stage = 2;
            ctx.request_repaint();
        } else if self.startup_stage < 35 {
            // Wait for ~35 frames to let background windows (Bubble/Tray) settle
            self.startup_stage += 1;
            ctx.request_repaint();
        } else if self.startup_stage == 35 {
            // CRITICAL: Wait for Tray Icon to be ready before starting splash
            // This ensures all shell integration is settled. Do not wait forever:
            // on some Windows/VM setups tray icon init can fail or be delayed,
            // and the app would remain permanently invisible.
            if self.tray_icon.is_none() {
                self.tray_startup_wait_frames = self.tray_startup_wait_frames.saturating_add(1);
                if self.tray_startup_wait_frames < TRAY_STARTUP_GRACE_FRAMES {
                    ctx.request_repaint();
                    return;
                }
                crate::log_info!(
                    "[Startup] Tray icon unavailable after {} frames; continuing without blocking window",
                    self.tray_startup_wait_frames
                );
            } else {
                self.tray_startup_wait_frames = 0;
            }

            if self.config.start_in_tray && self.tray_icon.is_some() {
                // ENSURE HIDDEN: If starting in tray, we must stay invisible.
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.startup_stage = 38;
            } else {
                // CREATE SPLASH while window is still invisible.
                // This lets the splash render one frame to the backbuffer before
                // the window appears, preventing the native border flash.
                if self.splash.is_none() {
                    self.splash = Some(crate::gui::splash::SplashScreen::new(ctx));
                }

                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    WINDOW_WIDTH,
                    WINDOW_HEIGHT,
                )));

                self.startup_stage = 36;
            }

            ctx.request_repaint();
        } else if self.startup_stage == 36 {
            // Splash has rendered one invisible frame — now show the window using
            // safe native chrome first. This avoids the hidden transparent/
            // undecorated startup path that can leave the app invisible on some
            // Windows setups.
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            super::utils::show_main_window_native();
            self.startup_stage = 37;
            ctx.request_repaint();
        } else if self.startup_stage == 37 {
            // After one visible frame, restore the custom transparent frameless chrome.
            self.ensure_custom_chrome(ctx);
            self.startup_stage = 38;
            ctx.request_repaint();
        }
    }

    /// Called exactly once when the splash screen finishes its exit animation.
    fn on_splash_finished(&mut self, ctx: &egui::Context) {
        // Ensure the main window has focus after the splash exit animation.
        // The splash runs for several seconds; if the user interacted with
        // another window during that time the egui window may no longer be
        // the foreground window. Without an explicit focus request the first
        // click on any button would only activate the window instead of
        // triggering the action, requiring an unwanted second click.
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub(crate) fn update_bubble_sync(&mut self) {
        // --- FAVORITE BUBBLE SYNC (Change-Detection Only) ---
        // Only trigger show/hide when state actually changes to avoid per-frame overhead
        let current_has_favorites = self.config.presets.iter().any(|p| p.is_favorite);
        let current_bubble_enabled = self.config.show_favorite_bubble;

        let locale = LocaleText::get(&self.config.ui_language);
        let bubble_text = if current_has_favorites {
            locale.tray_favorite_bubble
        } else {
            locale.tray_favorite_bubble_disabled
        };

        self.tray_favorite_bubble_item.set_text(bubble_text);
        self.tray_favorite_bubble_item.set_enabled(true);

        // Detect state change
        let state_changed = current_bubble_enabled != self.last_bubble_enabled
            || current_has_favorites != self.last_has_favorites;

        if state_changed {
            self.last_bubble_enabled = current_bubble_enabled;
            self.last_has_favorites = current_has_favorites;

            if current_bubble_enabled {
                crate::overlay::favorite_bubble::show_favorite_bubble();
            } else {
                crate::overlay::favorite_bubble::hide_favorite_bubble();
            }
        }
    }

    pub(crate) fn update_splash(&mut self, ctx: &egui::Context) {
        if let Some(splash) = &mut self.splash {
            match splash.update(ctx) {
                crate::gui::splash::SplashStatus::Ongoing => {
                    // Do NOT return here. Continue to render main UI underneath.
                }
                crate::gui::splash::SplashStatus::Finished => {
                    self.splash = None;
                    self.on_splash_finished(ctx);
                }
            }
        }
    }

    pub(crate) fn check_restore_signal(&mut self, ctx: &egui::Context) {
        if RESTORE_SIGNAL.swap(false, Ordering::SeqCst) {
            self.restore_window(ctx);
        }
        if crate::overlay::translation_gummy::REQUEST_DISMISS_SPLASH.swap(false, Ordering::SeqCst) {
            self.splash = None;
        }
    }

    pub(crate) fn update_tips_logic(&mut self, ctx: &egui::Context) {
        let text = LocaleText::get(&self.config.ui_language);
        let now = ctx.input(|i| i.time);

        // Initialize timer on first run
        if self.tip_timer == 0.0 {
            self.tip_timer = now;
        }

        // Calculate duration based on text length (reading speed ~ 15 chars/sec + 2s base)
        let current_tip = text
            .tips_list
            .get(self.current_tip_idx)
            .unwrap_or(&"")
            .to_string();
        let display_duration = (2.0 + (current_tip.len() as f64 * 0.06)) as f32;
        let fade_duration = 0.5f32;

        let elapsed = (now - self.tip_timer) as f32;

        if self.tip_is_fading_in {
            // Fading In
            self.tip_fade_state = (elapsed / fade_duration).min(1.0);
            if elapsed >= fade_duration {
                self.tip_fade_state = 1.0;
                // Fully visible, wait for duration
                if elapsed >= fade_duration + display_duration {
                    self.tip_is_fading_in = false; // Start fading out
                    self.tip_timer = now; // Reset timer for fade-out
                }
            }
            if elapsed < fade_duration {
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                let remaining = (fade_duration + display_duration - elapsed).max(0.0);
                ctx.request_repaint_after(Duration::from_secs_f32(remaining.max(0.016)));
            }
        } else {
            // Fading Out
            self.tip_fade_state = (1.0 - (elapsed / fade_duration)).max(0.0);
            if elapsed >= fade_duration {
                self.tip_fade_state = 0.0;

                // Switch to next random tip
                self.rng_seed = simple_rand(self.rng_seed);
                if !text.tips_list.is_empty() {
                    let next = (self.rng_seed as usize) % text.tips_list.len();
                    // Avoid repeating same tip if possible
                    if next == self.current_tip_idx && text.tips_list.len() > 1 {
                        self.current_tip_idx = (next + 1) % text.tips_list.len();
                    } else {
                        self.current_tip_idx = next;
                    }
                }

                self.tip_timer = now; // Reset timer
                self.tip_is_fading_in = true; // Start fading in
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    pub(crate) fn handle_events(&mut self, ctx: &egui::Context) {
        // --- Event Handling ---
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                UserEvent::Tray(tray_event) => {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = tray_event
                    {
                        self.restore_window(ctx);
                    }
                }
                UserEvent::Menu(menu_event) => {
                    match menu_event.id.0.as_str() {
                        "1002" => {
                            self.restore_window(ctx);
                        }
                        "1003" => {
                            // Toggle favorite bubble
                            self.config.show_favorite_bubble = !self.config.show_favorite_bubble;
                            self.tray_favorite_bubble_item
                                .set_checked(self.config.show_favorite_bubble);
                            self.save_and_sync();

                            // Spawn or dismiss the bubble overlay
                            if self.config.show_favorite_bubble {
                                crate::overlay::favorite_bubble::show_favorite_bubble();
                            } else {
                                crate::overlay::favorite_bubble::hide_favorite_bubble();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub(crate) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.is_quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }
}
