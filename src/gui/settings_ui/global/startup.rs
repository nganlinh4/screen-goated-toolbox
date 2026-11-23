use crate::admin_startup::{Issue, Mode};
use crate::config::Config;
use crate::gui::locale::LocaleText;
use crate::gui::startup::Startup;
use eframe::egui;

pub(super) fn render(
    ui: &mut egui::Ui,
    config: &mut Config,
    run_at_startup: &mut bool,
    startup: &mut Startup,
    administrator: bool,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let theme = crate::gui::theme::AppTheme::from_dark(ui.visuals().dark_mode);
    ui.add_enabled_ui(
        !startup.busy() && !crate::gui::app::has_pending_exit_request(),
        |ui| {
            if startup.available() {
                let mut enabled = *run_at_startup;
                if ui
                    .checkbox(&mut enabled, text.preset_basics.startup_label)
                    .changed()
                {
                    let mode = if !enabled {
                        Mode::Off
                    } else if config.run_as_admin_on_startup {
                        Mode::Elevated
                    } else {
                        Mode::Normal
                    };
                    startup.request(config, *run_at_startup, mode, ui.ctx());
                }
            }
            if *run_at_startup || config.run_as_admin_on_startup {
                ui.indent("admin_indent", |ui| {
                    let mut elevated = config.run_as_admin_on_startup;
                    let response = ui.add_enabled(
                        (administrator || elevated) && startup.available() && !startup.busy(),
                        egui::Checkbox::new(&mut elevated, text.desktop_settings.admin_startup_on),
                    );
                    if response.changed() {
                        let mode = if elevated {
                            Mode::Elevated
                        } else {
                            Mode::Normal
                        };
                        startup.request(config, *run_at_startup, mode, ui.ctx());
                    }
                    if !administrator {
                        ui.label(
                            egui::RichText::new(text.desktop_settings.admin_startup_fail)
                                .size(11.0)
                                .color(theme.warning()),
                        );
                    }
                    if *run_at_startup
                        && config.run_as_admin_on_startup
                        && !startup.busy()
                        && startup.issue().is_none()
                    {
                        ui.label(
                            egui::RichText::new(text.desktop_settings.admin_startup_success)
                                .size(11.0)
                                .color(theme.success()),
                        );
                    }
                });
            }
            if *run_at_startup
                && ui
                    .add_enabled(
                        !startup.busy(),
                        egui::Checkbox::new(
                            &mut config.start_in_tray,
                            text.desktop_settings.start_in_tray_label,
                        ),
                    )
                    .changed()
            {
                changed = true;
            }
        },
    );
    if startup.busy() {
        ui.spinner();
    }
    if let Some(issue) = startup.issue() {
        let message = match issue {
            Issue::AdministratorRequired => text.desktop_settings.admin_startup_fail,
            Issue::ProtectedLocationRequired => {
                text.desktop_settings.admin_startup_protected_location
            }
            Issue::TaskConflict => text.desktop_settings.admin_startup_conflict,
            Issue::OperationFailed => text.desktop_settings.admin_startup_operation_failed,
            Issue::UncertainState => text.desktop_settings.admin_startup_uncertain,
            Issue::PathTooLong => text.desktop_settings.admin_startup_path_too_long,
        };
        ui.label(
            egui::RichText::new(message)
                .size(11.0)
                .color(theme.warning()),
        );
    }
    changed
}
