use eframe::egui;

impl super::SettingsApp {
    pub(super) fn poll_startup(&mut self, ctx: &egui::Context) {
        if crate::gui::resize_subclass::is_live_resize() {
            if self.startup.busy() {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            return;
        }
        let Some(completion) = self
            .startup
            .poll(&mut self.config, &mut self.run_at_startup)
        else {
            return;
        };
        if let Some(selection) = completion.restore {
            if crate::gui::settings_ui::global::complete_restore(
                ctx,
                &mut self.config,
                selection,
                &mut self.run_at_startup,
            ) {
                self.save_and_sync();
            }
        } else if completion.changed {
            self.save_and_sync();
        } else if completion.restore_failed {
            let text = crate::gui::locale::LocaleText::get(&self.config.ui_language);
            crate::overlay::auto_copy_badge::show_notification(
                text.desktop_settings.admin_startup_operation_failed,
            );
        }
    }
}
