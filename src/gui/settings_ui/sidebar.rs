use eframe::egui;
use crate::config::{Config, Preset, ThemeMode};
use crate::gui::locale::LocaleText;
use crate::gui::icons::{Icon, icon_button, draw_icon_static, icon_button_sized};
use super::ViewMode;

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let mut preset_idx_to_delete = None;
    let mut preset_to_add_type = None;
    let mut preset_idx_to_select: Option<usize> = None;

    // Split indices for presets
    let mut img_indices = Vec::new();
    let mut other_indices = Vec::new();

    for (i, p) in config.presets.iter().enumerate() {
        if p.preset_type == "image" {
            img_indices.push(i);
        } else {
            other_indices.push(i);
        }
    }
    
    // Sort other indices: Text -> Audio -> Video -> Other
    other_indices.sort_by_key(|&i| {
        match config.presets[i].preset_type.as_str() {
            "text" => 0,
            "audio" => 1,
            "video" => 2,
            _ => 3,
        }
    });

    // Capture current view_mode for comparison (avoids borrow issues)
    let current_view_mode = view_mode.clone();
    let mut should_set_global = false;
    let mut should_set_history = false;

    // === UNIFIED GRID: Header + Presets in ONE grid for perfect alignment ===
    egui::Grid::new("sidebar_unified_grid")
        .striped(false)
        .spacing(egui::vec2(10.0, 6.0))
        .min_row_height(24.0) // Ensure consistent row heights
        .show(ui, |ui| {
            // === ROW 1: Theme/Lang/History | Global Settings ===
            // Column 1: Use explicit center alignment
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                
                // Theme Switcher
                let (theme_icon, tooltip) = match config.theme_mode {
                    ThemeMode::Dark => (Icon::Moon, "Theme: Dark"),
                    ThemeMode::Light => (Icon::Sun, "Theme: Light"),
                    ThemeMode::System => (Icon::SystemTheme, "Theme: System (Auto)"),
                };
                if icon_button(ui, theme_icon).on_hover_text(tooltip).clicked() {
                    config.theme_mode = match config.theme_mode {
                        ThemeMode::System => ThemeMode::Dark,
                        ThemeMode::Dark => ThemeMode::Light,
                        ThemeMode::Light => ThemeMode::System,
                    };
                    changed = true;
                }
                
                // Language Switcher
                let original_lang = config.ui_language.clone();
                let lang_display = match config.ui_language.as_str() {
                    "vi" => "VI",
                    "ko" => "KO",
                    _ => "EN",
                };
                egui::ComboBox::from_id_source("header_lang_switch")
                    .width(60.0)
                    .selected_text(lang_display)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut config.ui_language, "en".to_string(), "English");
                        ui.selectable_value(&mut config.ui_language, "vi".to_string(), "Vietnamese");
                        ui.selectable_value(&mut config.ui_language, "ko".to_string(), "Korean");
                    });
                if original_lang != config.ui_language {
                    changed = true;
                }
                
                // History Button
                if ui.button(text.history_btn).clicked() {
                    should_set_history = true;
                }
            });

            // Column 2: Global Settings (Right Aligned with Center vertical alignment)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let is_global = matches!(current_view_mode, ViewMode::Global);
                if ui.selectable_label(is_global, text.global_settings).clicked() {
                    should_set_global = true;
                }
                draw_icon_static(ui, Icon::Settings, None);
            });
            ui.end_row();

            // === ROW 3: Presets Title | Add Buttons ===
            ui.label(egui::RichText::new(text.presets_section).strong());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.style_mut().spacing.item_spacing.x = 4.0;
                // Buttons in reverse order for right_to_left
                if ui.button(text.add_audio_preset_btn).clicked() {
                    preset_to_add_type = Some("audio");
                }
                if ui.button(text.add_image_preset_btn).clicked() {
                    preset_to_add_type = Some("image");
                }
                if ui.button(text.add_text_preset_btn).clicked() {
                    preset_to_add_type = Some("text");
                }
            });
            ui.end_row();

            // === ROW 4+: Preset Items ===
            let max_len = std::cmp::max(img_indices.len(), other_indices.len());

            for i in 0..max_len {
                // Column 1: Image Presets
                if let Some(&idx) = img_indices.get(i) {
                    render_preset_item(ui, &config.presets, idx, &current_view_mode, &mut preset_idx_to_select, &mut preset_idx_to_delete);
                } else {
                    ui.label("");
                }

                // Column 2: Text/Audio/Other Presets
                if let Some(&idx) = other_indices.get(i) {
                    render_preset_item(ui, &config.presets, idx, &current_view_mode, &mut preset_idx_to_select, &mut preset_idx_to_delete);
                } else {
                    ui.label(""); 
                }
                
                ui.end_row();
            }
        });

    // Apply view_mode changes after the grid
    if should_set_global {
        *view_mode = ViewMode::Global;
    }
    if should_set_history {
        *view_mode = ViewMode::History;
    }
    if let Some(idx) = preset_idx_to_select {
        *view_mode = ViewMode::Preset(idx);
    }

    // Handle Add
    if let Some(type_str) = preset_to_add_type {
        let mut new_preset = Preset::default();
        if type_str == "text" {
            new_preset.preset_type = "text".to_string();
            new_preset.name = format!("Text {}", config.presets.len() + 1);
            new_preset.text_input_mode = "select".to_string();
            // Update block 0 instead of legacy fields
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "text".to_string();
                block.model = "text_accurate_kimi".to_string();
                block.prompt = "Translate this text.".to_string();
            }
        } else if type_str == "audio" {
            new_preset.preset_type = "audio".to_string();
            new_preset.name = format!("Audio {}", config.presets.len() + 1);
            new_preset.audio_source = "mic".to_string();
            // Update block 0 instead of legacy fields
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "audio".to_string();
                block.model = "whisper-fast".to_string();
                block.prompt = "".to_string();
            }
        } else {
            new_preset.name = format!("Image {}", config.presets.len() + 1);
            // Default preset already has image block, no changes needed
        }
        
        config.presets.push(new_preset);
        *view_mode = ViewMode::Preset(config.presets.len() - 1);
        changed = true;
    }

    // Handle Delete
    if let Some(idx) = preset_idx_to_delete {
        config.presets.remove(idx);
        if let ViewMode::Preset(curr) = *view_mode {
            if curr >= idx && curr > 0 {
                *view_mode = ViewMode::Preset(curr - 1);
            } else if config.presets.is_empty() {
                *view_mode = ViewMode::Global;
            } else {
                *view_mode = ViewMode::Preset(0);
            }
        }
        changed = true;
    }

    changed
}

// Helper function to render a single preset item (avoids closure borrow issues)
fn render_preset_item(
    ui: &mut egui::Ui,
    presets: &[Preset],
    idx: usize,
    current_view_mode: &ViewMode,
    preset_idx_to_select: &mut Option<usize>,
    preset_idx_to_delete: &mut Option<usize>,
) {
    let preset = &presets[idx];
    
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        
        let is_selected = matches!(current_view_mode, ViewMode::Preset(i) if *i == idx);
        
        let icon_type = match preset.preset_type.as_str() {
            "audio" => Icon::Microphone,
            "video" => Icon::Video,
            "text" => Icon::Text,
            _ => Icon::Image,
        };
        
        if preset.is_upcoming {
            ui.add_enabled_ui(false, |ui| {
                draw_icon_static(ui, icon_type, Some(14.0));
                let _ = ui.selectable_label(is_selected, &preset.name);
            });
        } else {
            draw_icon_static(ui, icon_type, Some(14.0));
            if ui.selectable_label(is_selected, &preset.name).clicked() {
                *preset_idx_to_select = Some(idx);
            }
            // Delete button (Small X icon)
            if presets.len() > 1 {
                if icon_button_sized(ui, Icon::Delete, 22.0).clicked() {
                    *preset_idx_to_delete = Some(idx);
                }
            }
        }
    });
}
