use crate::gui::icons::Icon;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::compact_filled_icon_button;
use eframe::egui;

pub(crate) const FOOTER_HORIZONTAL_MARGIN: i8 = 10;

pub(crate) fn footer_minimum_window_width(content_width: f32) -> f32 {
    let footer_width = content_width + f32::from(FOOTER_HORIZONTAL_MARGIN) * 2.0;
    crate::MIN_WINDOW_WIDTH.max(footer_width.ceil())
}

pub fn render_footer(ui: &mut egui::Ui, text: &LocaleText, toggles: FooterToggles<'_>) -> f32 {
    let FooterToggles {
        show_computer_control,
        show_screen_translate,
        show_pointer_gallery,
        show_translation_gummy,
        show_tts_playground,
        show_download,
    } = toggles;

    let is_dark = ui.visuals().dark_mode;
    // Bright accent fills read better with near-black labels in dark mode.
    let btn_text = if is_dark {
        egui::Color32::from_rgb(22, 22, 26)
    } else {
        egui::Color32::WHITE
    };
    let screen_translate_label = text.screen_translate.screen_translate_btn;
    let screen_translate_label_height = ui
        .painter()
        .layout_no_wrap(
            screen_translate_label.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            btn_text,
        )
        .rect
        .height();
    let row_height = ui
        .spacing()
        .interact_size
        .y
        .max(screen_translate_label_height + ui.spacing().button_padding.y * 2.0);

    let row = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let content_left = ui.cursor().left();
            let theme = AppTheme::from_ui(ui);
            ui.spacing_mut().button_padding.x = 3.0;
            ui.spacing_mut().item_spacing.x = 3.0;

            let computer_control_response = compact_filled_icon_button(
                ui,
                Icon::SmartToy,
                text.shell.computer_control_btn,
                theme.launch_computer_control(),
                btn_text,
                6,
            );
            if computer_control_response.clicked() {
                *show_computer_control = true;
            }
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("footer_first_launcher_test_rect"),
                    computer_control_response.rect,
                );
            });

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

            let screen_translate_response = compact_filled_icon_button(
                ui,
                Icon::Translate,
                screen_translate_label,
                theme.launch_screen_translate(),
                btn_text,
                6,
            )
            .on_hover_text(text.screen_translate.screen_translate_btn);
            if screen_translate_response.clicked() {
                *show_screen_translate = true;
            }
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("footer_screen_translate_test_rect"),
                    screen_translate_response.rect,
                );
            });

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

            if crate::creation_feature_availability::image_to_svg_entry_visible()
                && compact_filled_icon_button(
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

            if crate::creation_feature_availability::image_creator_entry_visible()
                && compact_filled_icon_button(
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

            (ui.cursor().left() - content_left).max(0.0)
        },
    );
    row.inner
}

pub struct FooterToggles<'a> {
    pub show_computer_control: &'a mut bool,
    pub show_screen_translate: &'a mut bool,
    pub show_pointer_gallery: &'a mut bool,
    pub show_translation_gummy: &'a mut bool,
    pub show_tts_playground: &'a mut bool,
    pub show_download: &'a mut bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_localized_launchers_stay_expanded_on_one_row_at_minimum_width() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4_096.0, 100.0));
        let mut localized_widths = Vec::new();
        for language in ["en", "vi", "ko"] {
            let context = egui::Context::default();
            crate::gui::configure_fonts(&context);
            AppTheme::apply_global_style(&context, false);
            let text = LocaleText::get(language);
            let mut show_computer_control = false;
            let mut show_screen_translate = false;
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
                        .inner_margin(egui::Margin::symmetric(FOOTER_HORIZONTAL_MARGIN, 4))
                        .show(ui, |ui| {
                            let content_width = render_footer(
                                ui,
                                &text,
                                FooterToggles {
                                    show_computer_control: &mut show_computer_control,
                                    show_screen_translate: &mut show_screen_translate,
                                    show_pointer_gallery: &mut show_pointer_gallery,
                                    show_translation_gummy: &mut show_translation_gummy,
                                    show_tts_playground: &mut show_tts_playground,
                                    show_download: &mut show_download,
                                },
                            );
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::new("footer_content_width_test_value"),
                                    content_width,
                                );
                            });
                        });
                },
            );

            let first_launcher_rect = context
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("footer_first_launcher_test_rect"))
                })
                .expect("first footer launcher rect should be captured");
            let last_launcher_rect = context
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("footer_last_launcher_test_rect"))
                })
                .expect("last footer launcher rect should be captured");
            let screen_translate_rect = context
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("footer_screen_translate_test_rect"))
                })
                .expect("Screen Translate footer rect should be captured");
            let content_width = context
                .data(|data| data.get_temp::<f32>(egui::Id::new("footer_content_width_test_value")))
                .expect("footer content width should be captured");
            assert!(
                first_launcher_rect.width()
                    > crate::gui::icons::ICON_MD + ui_horizontal_button_chrome(),
                "{language} first launcher collapsed to an icon: {first_launcher_rect:?}"
            );
            assert!(
                (first_launcher_rect.center().y - last_launcher_rect.center().y).abs() <= 1.0,
                "{language} launchers left one row: first={first_launcher_rect:?}, last={last_launcher_rect:?}"
            );
            assert!(
                (screen_translate_rect.height() - first_launcher_rect.height()).abs() <= 1.0,
                "{language} Screen Translate label wrapped onto another row: screen_translate={screen_translate_rect:?}, launcher={first_launcher_rect:?}"
            );
            assert!(
                content_width + 1.0 >= last_launcher_rect.right() - first_launcher_rect.left(),
                "{language} measured footer width excludes a launcher: width={content_width}, first={first_launcher_rect:?}, last={last_launcher_rect:?}"
            );
            let minimum_width = footer_minimum_window_width(content_width);
            assert!(
                minimum_width + 1.0
                    >= last_launcher_rect.right() + f32::from(FOOTER_HORIZONTAL_MARGIN),
                "{language} footer minimum leaves its last launcher outside the viewport: minimum={minimum_width}, last={last_launcher_rect:?}"
            );
            localized_widths.push(content_width);
        }
        assert!(
            localized_widths
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 1.0),
            "localized footer labels should produce locale-specific minimum widths: {localized_widths:?}"
        );
    }

    const fn ui_horizontal_button_chrome() -> f32 {
        3.0 * 2.0
    }
}
