use eframe::egui;
use crate::config::Config;
use crate::gui::locale::LocaleText;
use crate::gui::icons::{Icon, icon_button, draw_icon_static};
use crate::history::{HistoryManager, HistoryItem, HistoryType};

pub fn render_history_panel(
    ui: &mut egui::Ui,
    config: &mut Config,
    history_manager: &HistoryManager,
    search_query: &mut String,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    
    let is_dark = ui.visuals().dark_mode;
    let card_bg = if is_dark {
        egui::Color32::from_rgba_unmultiplied(28, 32, 42, 250)  // Darker for better text contrast
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255)
    };
    let card_stroke = if is_dark {
        egui::Stroke::new(1.0, egui::Color32::from_gray(50))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(210))
    };
    
    // Set max width for entire panel (outside frame so it properly constrains the card)
    ui.set_max_width(510.0);
    
    // === HEADER CARD ===  
    ui.add_space(5.0);
    egui::Frame::none()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            // Row 1: Title + Max items slider
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("📜 {}", text.history_title)).strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Slider::new(&mut config.max_history_items, 10..=200)).changed() {
                        history_manager.request_prune(config.max_history_items);
                        changed = true;
                    }
                    ui.label(text.max_items_label);
                });
            });
            
            ui.add_space(8.0);
            
            // Row 2: Search + Actions
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(search_query).hint_text(text.search_placeholder).desired_width(220.0));
                
                if !search_query.is_empty() {
                    if icon_button(ui, Icon::Close).on_hover_text("Clear search").clicked() {
                        *search_query = "".to_string(); 
                    }
                }
                
                if icon_button(ui, Icon::Folder).on_hover_text("Open Media Folder").clicked() {
                    let config_dir = dirs::config_dir().unwrap_or_default().join("screen-goated-toolbox").join("history_media");
                    let _ = std::fs::create_dir_all(&config_dir);
                    let _ = open::that(config_dir);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Clear All button - styled
                    let clear_bg = if is_dark { 
                        egui::Color32::from_rgb(120, 60, 60) 
                    } else { 
                        egui::Color32::from_rgb(220, 140, 140) 
                    };
                    if ui.add(egui::Button::new(egui::RichText::new(text.clear_all_history_btn).color(egui::Color32::WHITE).small())
                        .fill(clear_bg)
                        .corner_radius(8.0))
                        .clicked() {
                        history_manager.clear_all();
                    }
                });
            });
        });
    
    ui.add_space(8.0);
    
    let items = history_manager.items.lock().unwrap().clone();
    let q = search_query.to_lowercase();
    let filtered: Vec<&HistoryItem> = items.iter().filter(|i| {
        q.is_empty() || i.text.to_lowercase().contains(&q) || i.timestamp.contains(&q)
    }).collect();

    if filtered.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(text.history_empty);
        });
    } else {
        // History items in scroll area
        egui::Frame::none().show(ui, |ui| {
            ui.set_height(460.0);
            
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(510.0);
                
                let mut id_to_delete = None;
                
                for item in filtered {
                    // Distinct but subtle colors based on item type
                    let item_bg = match item.item_type {
                        HistoryType::Image => if is_dark {
                            // Subtle blue tint for images
                            egui::Color32::from_rgba_unmultiplied(30, 38, 52, 235)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(240, 245, 255, 255)
                        },
                        HistoryType::Text => if is_dark {
                            // Subtle green tint for text
                            egui::Color32::from_rgba_unmultiplied(30, 42, 38, 235)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(240, 252, 245, 255)
                        },
                        HistoryType::Audio => if is_dark {
                            // Subtle orange/amber tint for audio
                            egui::Color32::from_rgba_unmultiplied(42, 36, 30, 235)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 250, 240, 255)
                        },
                    };
                    
                    egui::Frame::none()
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
                                draw_icon_static(ui, icon, Some(14.0));
                                ui.label(egui::RichText::new(&item.timestamp).size(10.0).weak());
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if icon_button(ui, Icon::DeleteLarge).on_hover_text("Delete").clicked() {
                                        id_to_delete = Some(item.id);
                                    }
                                    
                                    if icon_button(ui, Icon::Copy).on_hover_text("Copy Text").clicked() {
                                        crate::gui::utils::copy_to_clipboard_text(&item.text);
                                    }

                                    if item.item_type != HistoryType::Text {
                                        let btn_text = match item.item_type {
                                            HistoryType::Image => text.view_image_btn,
                                            HistoryType::Audio => text.listen_audio_btn,
                                            HistoryType::Text => "",
                                        };
                                        if ui.button(btn_text).clicked() {
                                            let config_dir = dirs::config_dir().unwrap().join("screen-goated-toolbox").join("history_media");
                                            let path = config_dir.join(&item.media_path);
                                            let _ = open::that(path);
                                        }
                                    }
                                });
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
