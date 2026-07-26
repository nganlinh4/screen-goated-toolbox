use crate::gui::icons::{Icon, paint_icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::compact_filled_icon_button;
use eframe::egui;

pub fn render_footer(ui: &mut egui::Ui, text: &LocaleText, toggles: FooterToggles<'_>) {
    let FooterToggles {
        show_modal,
        show_computer_control,
        show_pointer_gallery,
        show_translation_gummy,
        show_tts_playground,
        show_download,
    } = toggles;

    ui.horizontal(|ui| {
        let theme = AppTheme::from_ui(ui);
        let is_dark = ui.visuals().dark_mode;
        // Bright accent fills read better with near-black labels in dark mode.
        let btn_text = if is_dark {
            egui::Color32::from_rgb(22, 22, 26)
        } else {
            egui::Color32::WHITE
        };
        ui.spacing_mut().item_spacing.x = 6.0;

        if compact_filled_icon_button(
            ui,
            Icon::SmartToy,
            text.shell.computer_control_btn,
            theme.launch_computer_control(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_computer_control = true;
        }

        if compact_filled_icon_button(
            ui,
            Icon::Pointer,
            text.tool_runtime.pointer_gallery_btn,
            theme.launch_pointer(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_pointer_gallery = true;
        }

        if compact_filled_icon_button(
            ui,
            Icon::BreakfastDining,
            text.translation_gummy.translation_gummy_btn,
            theme.launch_translation(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_translation_gummy = true;
        }

        if compact_filled_icon_button(
            ui,
            Icon::Speaker,
            text.tts_playground.tts_playground_btn,
            theme.launch_tts(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_tts_playground = true;
        }

        if compact_filled_icon_button(
            ui,
            Icon::DeployedCode,
            text.shell.three_d_generator_btn,
            theme.accent_three_d_generator(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::three_d_generator::show_three_d_generator();
        }

        if compact_filled_icon_button(
            ui,
            Icon::DrawCollage,
            text.shell.image_to_svg_btn,
            theme.accent_image_to_svg(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::image_to_svg::show_image_to_svg();
        }

        if compact_filled_icon_button(
            ui,
            Icon::Image,
            text.shell.image_creator_btn,
            theme.accent_image_creator(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::image_creator::show_image_creator();
        }

        if compact_filled_icon_button(
            ui,
            Icon::Album,
            text.shell.prompt_dj_btn,
            theme.accent_prompt_dj(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::prompt_dj::show_prompt_dj();
        }

        if compact_filled_icon_button(
            ui,
            Icon::Movie,
            text.auxiliary.download.download_feature_btn,
            theme.accent_download(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_download = true;
        }

        let screen_record_response = compact_filled_icon_button(
            ui,
            Icon::Videocam,
            text.tool_runtime.screen_record_btn,
            theme.accent_screen_record(),
            btn_text,
            6,
        );
        if screen_record_response.clicked() {
            crate::overlay::screen_record::show_screen_record();
        }

        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("footer_last_launcher_test_rect"),
                screen_record_response.rect,
            );
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tips_button(ui, text).clicked() {
                *show_modal = true;
            }
        });
    });
}

pub struct FooterToggles<'a> {
    pub show_modal: &'a mut bool,
    pub show_computer_control: &'a mut bool,
    pub show_pointer_gallery: &'a mut bool,
    pub show_translation_gummy: &'a mut bool,
    pub show_tts_playground: &'a mut bool,
    pub show_download: &'a mut bool,
}

fn tips_button(ui: &mut egui::Ui, text: &LocaleText) -> egui::Response {
    let text_color = ui.visuals().text_color();
    let label_galley = ui.painter().layout_no_wrap(
        text.workspace.tips_btn.to_owned(),
        egui::TextStyle::Button.resolve(ui.style()),
        text_color,
    );
    let icon_size = crate::gui::icons::ICON_SM;
    let icon_gap = 4.0;
    let horizontal_padding = 6.0;
    let button_size = egui::vec2(
        horizontal_padding * 2.0 + icon_size + icon_gap + label_galley.rect.width(),
        ui.spacing()
            .interact_size
            .y
            .max(label_galley.rect.height() + ui.spacing().button_padding.y * 2.0),
    );
    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

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
    paint_icon(
        ui.painter(),
        icon_rect,
        Icon::Lightbulb,
        AppTheme::from_ui(ui).warning(),
    );
    ui.painter().galley(
        egui::pos2(
            icon_rect.right() + icon_gap,
            rect.center().y - label_galley.rect.height() / 2.0,
        ),
        label_galley,
        text_color,
    );

    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(text.workspace.tips_click_hint);
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new("footer_tips_test_rect"), response.rect);
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_tips_action_stays_on_the_launcher_row_in_every_locale() {
        let screen =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(crate::MIN_WINDOW_WIDTH, 100.0));

        for (language, expected_label) in [("en", "Tips"), ("vi", "Mẹo"), ("ko", "팁")] {
            let context = egui::Context::default();
            crate::gui::configure_fonts(&context);
            AppTheme::apply_global_style(&context, false);
            let text = LocaleText::get(language);
            assert_eq!(text.workspace.tips_btn, expected_label);
            let mut show_modal = false;
            let mut show_computer_control = false;
            let mut show_pointer_gallery = false;
            let mut show_translation_gummy = false;
            let mut show_tts_playground = false;
            let mut show_download = false;

            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| {
                    egui::Frame::default()
                        .inner_margin(egui::Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            render_footer(
                                ui,
                                &text,
                                FooterToggles {
                                    show_modal: &mut show_modal,
                                    show_computer_control: &mut show_computer_control,
                                    show_pointer_gallery: &mut show_pointer_gallery,
                                    show_translation_gummy: &mut show_translation_gummy,
                                    show_tts_playground: &mut show_tts_playground,
                                    show_download: &mut show_download,
                                },
                            );
                        });
                },
            );

            let last_launcher_rect = context
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("footer_last_launcher_test_rect"))
                })
                .expect("last footer launcher rect should be captured");
            let tips_rect = context
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("footer_tips_test_rect")))
                .expect("tips action rect should be captured");
            assert!(
                last_launcher_rect.right() + 6.0 <= tips_rect.left(),
                "{language} tips action overlapped the launchers: last={last_launcher_rect:?}, tips={tips_rect:?}"
            );
            assert!(
                (last_launcher_rect.center().y - tips_rect.center().y).abs() <= 1.0,
                "{language} tips action left the launcher row: last={last_launcher_rect:?}, tips={tips_rect:?}"
            );
            assert!(
                screen.contains_rect(tips_rect),
                "{language} tips action overflowed the minimum viewport: {tips_rect:?}"
            );
        }
    }
}
