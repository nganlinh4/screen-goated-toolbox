use crate::gui::icons::{self, Icon};
use crate::gui::locale::WorkspaceLocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

pub(super) fn render_tips_entry_button(
    ui: &mut egui::Ui,
    text: &WorkspaceLocaleText,
) -> egui::Response {
    let enabled = text
        .tips_sections
        .iter()
        .any(|section| !section.tips.is_empty());
    let text_color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let label_galley = ui.painter().layout_no_wrap(
        text.tips_btn.to_owned(),
        egui::TextStyle::Button.resolve(ui.style()),
        text_color,
    );
    let icon_size = icons::ICON_SM;
    let icon_gap = 4.0;
    let horizontal_padding = 6.0;
    let button_size = egui::vec2(
        horizontal_padding * 2.0 + icon_size + icon_gap + label_galley.rect.width(),
        ui.spacing()
            .interact_size
            .y
            .max(label_galley.rect.height() + ui.spacing().button_padding.y * 2.0),
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(button_size, sense);

    let state_fill = if response.is_pointer_button_down_on() {
        ui.visuals().widgets.active.weak_bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    if state_fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, state_fill);
    }

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + horizontal_padding,
            rect.center().y - icon_size / 2.0,
        ),
        egui::vec2(icon_size, icon_size),
    );
    icons::paint_icon(
        ui.painter(),
        icon_rect,
        Icon::Lightbulb,
        if enabled {
            AppTheme::from_ui(ui).warning()
        } else {
            ui.visuals().weak_text_color()
        },
    );
    ui.painter().galley(
        egui::pos2(
            icon_rect.right() + icon_gap,
            rect.center().y - label_galley.rect.height() / 2.0,
        ),
        label_galley,
        text_color,
    );

    let response = if enabled {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(text.tips_click_hint)
    } else {
        response
    };
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new("footer_tips_entry_test_rect"), response.rect);
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_entry_is_localized_and_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/mobile-shell/usage-tips.json"
        ))
        .expect("valid usage-tips fixture");
        let windows_case = fixture["cases"]
            .as_array()
            .expect("fixture cases")
            .iter()
            .find(|case| case["name"] == "windows_static_entry_contract")
            .expect("Windows tips fixture case");
        assert_eq!(windows_case["entry_surface"], "footer_button");
        assert_eq!(windows_case["entry_placement"], "after_mini_app_launchers");

        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 40.0));
        for (language, expected_label) in [("en", "Tips"), ("vi", "Mẹo"), ("ko", "팁")] {
            let context = egui::Context::default();
            crate::gui::configure_fonts(&context);
            AppTheme::apply_global_style(&context, false);
            let locale = crate::gui::locale::LocaleText::get(language);
            assert_eq!(locale.workspace.tips_btn, expected_label);

            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        render_tips_entry_button(ui, &locale.workspace);
                    });
                },
            );

            let rect = context
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("footer_tips_entry_test_rect"))
                })
                .expect("footer Tips entry rect");
            assert!(
                screen.contains_rect(rect),
                "{language} Tips entry escaped its compact test surface: {rect:?}"
            );
        }
    }
}
