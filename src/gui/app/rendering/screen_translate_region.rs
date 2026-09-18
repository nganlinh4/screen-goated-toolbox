use super::super::types::SettingsApp;
use eframe::egui;

impl SettingsApp {
    pub(super) fn start_screen_translate_region_edit(&mut self, ctx: &egui::Context) {
        if self.screen_translate_region_edit.is_some()
            || crate::overlay::is_busy()
            || crate::overlay::is_selection_overlay_active()
        {
            return;
        }
        self.recording_screen_translate_hotkey = false;
        self.screen_translate_hotkey_conflict_msg = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                crate::gui::app::main_window_hwnd(),
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
        self.screen_translate_region_edit =
            Some(crate::overlay::screen_translate::fixed_region::edit(
                self.config.screen_translate.fixed_region.clone(),
                ctx.clone(),
            ));
    }

    pub(crate) fn poll_screen_translate_region_edit(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.screen_translate_region_edit else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(value) => value,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("region editor closed unexpectedly"))
            }
        };
        self.screen_translate_region_edit = None;
        match result {
            Ok(Some(region)) => {
                self.sync_global_hotkeys();
                self.config.screen_translate.fixed_region = Some(region);
                self.save_and_sync();
            }
            Ok(None) => {}
            Err(error) => crate::overlay::auto_copy_badge::show_notification(&error.to_string()),
        }
        self.restore_window(ctx);
    }
}
