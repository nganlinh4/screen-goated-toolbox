use crate::config::Config;
use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use crate::history::{HistoryItem, HistoryManager, HistoryType};
use eframe::egui;

pub fn render_history_panel(
    ui: &mut egui::Ui,
    config: &mut Config,
    history_manager: &HistoryManager,
    search_query: &mut String,
    text: &LocaleText,
    content_bottom: f32,
) -> bool {
    let mut changed = false;

    let is_dark = ui.visuals().dark_mode;
    let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
    let card_bg = theme.card_bg();
    let card_stroke = theme.card_stroke();

    // The panel (a native side-panel) already bounds the width; don't force a
    // hardcoded max that exceeds it (that overran the panel and clipped the text).

    // === HEADER CARD ===
    ui.add_space(5.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            // Row 1: Title + Max items slider
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::History, Some(crate::gui::icons::ICON_SM));
                ui.label(egui::RichText::new(text.history_title).strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Slider::new(&mut config.max_history_items, 10..=200))
                        .changed()
                    {
                        history_manager.request_prune(config.max_history_items);
                        changed = true;
                    }
                    ui.label(text.max_items_label);
                });
            });

            ui.add_space(6.0);

            // Computer Control conversation memory has its OWN retention cap so the
            // (numerous) media history items above can't push CC conversations out.
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::SmartToy, Some(crate::gui::icons::ICON_SM));
                ui.label(egui::RichText::new(text.cc_memory_max_label).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Slider::new(&mut config.cc_max_memory_items, 5..=100))
                        .changed()
                    {
                        changed = true;
                    }
                });
            });

            ui.add_space(8.0);

            // Row 2: Search + Actions
            ui.horizontal(|ui| {
                ui.scope(|ui| {
                    if !is_dark {
                        let visuals = ui.visuals_mut();
                        visuals.extreme_bg_color = egui::Color32::from_gray(242);
                        visuals.widgets.inactive.bg_stroke =
                            egui::Stroke::new(1.0, egui::Color32::from_gray(220));
                        visuals.widgets.hovered.bg_stroke =
                            egui::Stroke::new(1.0, egui::Color32::from_gray(200));
                        visuals.widgets.active.bg_stroke =
                            egui::Stroke::new(1.0, egui::Color32::from_gray(180));
                    }
                    ui.add(
                        egui::TextEdit::singleline(search_query)
                            .hint_text(text.search_placeholder)
                            .desired_width(220.0),
                    );
                });

                if !search_query.is_empty()
                    && icon_button(ui, Icon::Close)
                        .on_hover_text(text.history_clear_search_tooltip)
                        .clicked()
                {
                    *search_query = "".to_string();
                }

                if icon_button(ui, Icon::Folder)
                    .on_hover_text(text.history_open_media_folder_tooltip)
                    .clicked()
                {
                    let config_dir = crate::paths::app_config_dir().join("history_media");
                    let _ = std::fs::create_dir_all(&config_dir);
                    let _ = open::that(config_dir);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Clear All button — destructive action via the shared Material
                    // filled-button helper (keeps hover/press state layers).
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.clear_all_history_btn,
                        theme.danger_fill(),
                        theme.on_accent(),
                        8,
                    )
                    .clicked()
                    {
                        history_manager.clear_all();
                    }
                });
            });
        });

    ui.add_space(8.0);

    let items = history_manager.items.lock().unwrap().clone();
    let q = search_query.to_lowercase();
    let filtered: Vec<&HistoryItem> = items
        .iter()
        .filter(|i| q.is_empty() || i.text.to_lowercase().contains(&q) || i.timestamp.contains(&q))
        .collect();

    if filtered.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(text.history_empty);
        });
    } else {
        // History items in scroll area — fill the remaining height down to the
        // footer. Measure from the panel's true bottom (`content_bottom`); the
        // column's `available_height()` reports a capped value here.
        let list_h = (content_bottom - ui.cursor().top() - 16.0).max(300.0);
        egui::Frame::new().show(ui, |ui| {
            ui.set_height(list_h);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut id_to_delete = None;

                for item in filtered {
                    // Distinct but subtle colors based on item type
                    let item_bg = match item.item_type {
                        HistoryType::Image => {
                            if is_dark {
                                // Subtle blue tint for images
                                egui::Color32::from_rgba_unmultiplied(30, 38, 52, 235)
                            } else {
                                egui::Color32::from_rgba_unmultiplied(240, 245, 255, 255)
                            }
                        }
                        HistoryType::Text => {
                            if is_dark {
                                // Subtle green tint for text
                                egui::Color32::from_rgba_unmultiplied(30, 42, 38, 235)
                            } else {
                                egui::Color32::from_rgba_unmultiplied(240, 252, 245, 255)
                            }
                        }
                        HistoryType::Audio => {
                            if is_dark {
                                // Subtle orange/amber tint for audio
                                egui::Color32::from_rgba_unmultiplied(42, 36, 30, 235)
                            } else {
                                egui::Color32::from_rgba_unmultiplied(255, 250, 240, 255)
                            }
                        }
                    };

                    egui::Frame::new()
                        .fill(item_bg)
                        .stroke(card_stroke)
                        .inner_margin(8.0)
                        .corner_radius(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon = match item.item_type {
                                    HistoryType::Image => Icon::Image,
                                    HistoryType::Audio => Icon::Microphone,
                                    HistoryType::Text => Icon::Text,
                                };
                                draw_icon_static(ui, icon, Some(crate::gui::icons::ICON_SM));
                                ui.label(egui::RichText::new(&item.timestamp).size(10.0).weak());

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if icon_button(ui, Icon::DeleteLarge)
                                            .on_hover_text(text.history_delete_tooltip)
                                            .clicked()
                                        {
                                            id_to_delete = Some(item.id);
                                        }

                                        if icon_button(ui, Icon::Copy)
                                            .on_hover_text(text.history_copy_text_tooltip)
                                            .clicked()
                                        {
                                            crate::gui::utils::copy_to_clipboard_text(&item.text);
                                        }

                                        if !item.media_path.is_empty() {
                                            let btn_text = match item.item_type {
                                                HistoryType::Image => text.view_image_btn,
                                                HistoryType::Audio => text.listen_audio_btn,
                                                HistoryType::Text => text.view_text_btn,
                                            };
                                            if ui.button(btn_text).clicked() {
                                                let config_dir = crate::paths::app_config_dir()
                                                    .join("history_media");
                                                let path = config_dir.join(&item.media_path);
                                                let _ = open::that(path);
                                            }
                                        }
                                    },
                                );
                            });

                            ui.label(egui::RichText::new(&item.text).size(13.0));
                        });
                    ui.add_space(4.0);
                }

                if let Some(id) = id_to_delete {
                    history_manager.delete(id);
                }
            });
        });
    }

    changed
}
