use crate::config::{Config, ModelPriorityChains};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::model_selector;
use crate::gui::theme::AppTheme;
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

pub fn render_model_priority_modal(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    show_modal: &mut bool,
) -> bool {
    if !*show_modal {
        return false;
    }

    let theme = AppTheme::from_ui(ui);
    let mut changed = false;

    let modal = crate::gui::widgets::material_modal(
        ui.ctx(),
        &theme,
        egui::Id::new("model_priority_modal"),
        |ui| {
            ui.set_width(760.0);

            // Header: title + skip-hint description + close.
            if crate::gui::widgets::dialog_header(
                ui,
                &theme,
                text.model_catalog.model_priority_title,
                Some(text.model_catalog.model_priority_skip_hint),
                |_| {},
            ) {
                *show_modal = false;
            }

            ui.columns(2, |columns| {
                if render_chain_section(
                    &mut columns[0],
                    &mut config.model_priority_chains.image_to_text,
                    RetryChainKind::ImageToText,
                    &config.ui_language,
                    text,
                ) {
                    changed = true;
                }

                if render_chain_section(
                    &mut columns[1],
                    &mut config.model_priority_chains.text_to_text,
                    RetryChainKind::TextToText,
                    &config.ui_language,
                    text,
                ) {
                    changed = true;
                }
            });
        },
    );

    if modal.should_close() {
        *show_modal = false;
    }

    changed
}

fn render_chain_section(
    ui: &mut egui::Ui,
    chain: &mut Vec<String>,
    chain_kind: RetryChainKind,
    ui_language: &str,
    text: &LocaleText,
) -> bool {
    enum RowAction {
        None,
        MoveUp,
        MoveDown,
        Remove,
    }

    let mut changed = false;
    let section_title = match chain_kind {
        RetryChainKind::ImageToText => text.model_catalog.model_priority_image_chain_title,
        RetryChainKind::TextToText => text.model_catalog.model_priority_text_chain_title,
    };
    let section_id = match chain_kind {
        RetryChainKind::ImageToText => "model_priority_image_chain",
        RetryChainKind::TextToText => "model_priority_text_chain",
    };
    let theme = AppTheme::from_ui(ui);
    let section_title_color = match chain_kind {
        RetryChainKind::ImageToText => theme.node_special_title(),
        RetryChainKind::TextToText => theme.on_surface(),
    };

    ui.group(|ui| {
        ui.set_min_width(340.0);
        ui.horizontal(|ui| {
            crate::gui::icons::arrow_label(ui, section_title, Some(section_title_color), |rt| {
                rt.strong().size(13.0).color(section_title_color)
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(text.preset_basics.reset_defaults_btn)
                    .clicked()
                {
                    let defaults = ModelPriorityChains::default();
                    *chain = match chain_kind {
                        RetryChainKind::ImageToText => defaults.image_to_text,
                        RetryChainKind::TextToText => defaults.text_to_text,
                    };
                    changed = true;
                }
            });
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("1.");
            ui.label(egui::RichText::new(text.model_catalog.model_priority_chosen_model).strong());
            crate::gui::icons::draw_icon_static(
                ui,
                crate::gui::icons::Icon::ArrowRightAlt,
                Some(crate::gui::icons::ICON_SM),
            );
            ui.label(
                egui::RichText::new(text.model_catalog.model_priority_fixed_hint)
                    .small()
                    .weak(),
            );
        });
        ui.add_space(6.0);

        let mut row_idx = 0;
        while row_idx < chain.len() {
            let mut row_action = RowAction::None;
            ui.horizontal(|ui| {
                ui.label(format!("{}.", row_idx + 2));

                changed |= model_selector::render_model_combo(
                    ui,
                    (section_id, "combo", row_idx),
                    &mut chain[row_idx],
                    chain_kind,
                    ui_language,
                );

                if crate::gui::icons::icon_button_sized(
                    ui,
                    crate::gui::icons::Icon::ArrowUp,
                    crate::gui::icons::ICON_LG,
                )
                .clicked()
                    && row_idx > 0
                {
                    row_action = RowAction::MoveUp;
                }
                if crate::gui::icons::icon_button_sized(
                    ui,
                    crate::gui::icons::Icon::ArrowDown,
                    crate::gui::icons::ICON_LG,
                )
                .clicked()
                    && row_idx + 1 < chain.len()
                {
                    row_action = RowAction::MoveDown;
                }
                if crate::gui::icons::icon_button_sized(
                    ui,
                    crate::gui::icons::Icon::Close,
                    crate::gui::icons::ICON_LG,
                )
                .clicked()
                {
                    row_action = RowAction::Remove;
                }
            });

            match row_action {
                RowAction::MoveUp => {
                    chain.swap(row_idx, row_idx - 1);
                    changed = true;
                    row_idx = row_idx.saturating_sub(1);
                }
                RowAction::MoveDown => {
                    chain.swap(row_idx, row_idx + 1);
                    changed = true;
                    row_idx += 1;
                }
                RowAction::Remove => {
                    chain.remove(row_idx);
                    changed = true;
                    continue;
                }
                RowAction::None => {}
            }

            row_idx += 1;
        }

        ui.add_space(4.0);
        if ui
            .button(text.model_catalog.model_priority_add_model)
            .clicked()
        {
            chain.push(model_selector::default_model_id(chain_kind));
            changed = true;
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(format!("{}.", chain.len() + 2));
            ui.label(egui::RichText::new(text.model_catalog.model_priority_auto).strong());
            crate::gui::icons::draw_icon_static(
                ui,
                crate::gui::icons::Icon::ArrowRightAlt,
                Some(crate::gui::icons::ICON_SM),
            );
            ui.label(
                egui::RichText::new(text.model_catalog.model_priority_auto_hint)
                    .small()
                    .weak(),
            );
        });
    });

    changed
}
