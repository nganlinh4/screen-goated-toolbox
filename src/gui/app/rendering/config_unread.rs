//! Tells the user when the saved settings could not be read at startup. The
//! session then runs on defaults and refuses to save, so nothing changed in
//! it is kept.

use crate::gui::locale::LocaleText;
use eframe::egui;
use std::path::Path;

/// The warning to show for an unread config file, if there is one.
fn config_unread_warning(text: &LocaleText, unread_path: Option<&Path>) -> Option<String> {
    unread_path.map(|path| {
        text.shell
            .config_unread_warning
            .replace("{path}", &path.display().to_string())
    })
}

/// A top banner over the settings body while the config file is unread.
pub(super) fn render_config_unread_banner(root_ui: &mut egui::Ui, text: &LocaleText) {
    let unread_path = crate::config::unread_config_path();
    let Some(message) = config_unread_warning(text, unread_path.as_deref()) else {
        return;
    };
    egui::Panel::top("config_unread_banner")
        .show_separator_line(false)
        .frame(
            egui::Frame::NONE
                .fill(root_ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(
                    super::BODY_EDGE_INSET,
                    crate::gui::theme::space::HAIR,
                )),
        )
        .show(root_ui, |ui| {
            ui.colored_label(ui.visuals().error_fg_color, message);
        });
}

#[cfg(test)]
mod tests {
    use super::config_unread_warning;
    use crate::gui::locale::LocaleText;
    use std::path::Path;

    #[test]
    fn an_unread_config_is_announced_with_its_path_in_every_language() {
        let path = Path::new("C:/settings/config_v3.json");
        for lang in ["en", "vi", "ko"] {
            let text = LocaleText::get(lang);
            let warning = config_unread_warning(&text, Some(path)).expect(lang);
            assert!(warning.contains("C:/settings/config_v3.json"), "{lang}");
            assert!(!warning.contains('{'), "{lang}: {warning}");
            assert_eq!(config_unread_warning(&text, None), None, "{lang}");
        }
    }
}
