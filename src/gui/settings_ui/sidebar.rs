use eframe::egui;
use crate::config::{Config, Preset, ThemeMode};
use crate::gui::locale::LocaleText;
use crate::gui::icons::{Icon, icon_button, draw_icon_static};
use super::ViewMode;

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> bool {
    let mut changed = false;

    // Theme & Language Controls
    ui.horizontal(|ui| {
        // --- THEME SWITCHER LOGIC ---
        let (theme_icon, tooltip) = match config.theme_mode {
            ThemeMode::Dark => (Icon::Moon, "Theme: Dark"),
            ThemeMode::Light => (Icon::Sun, "Theme: Light"),
            ThemeMode::System => (Icon::SystemTheme, "Theme: System (Auto)"),
        };

        if icon_button(ui, theme_icon).on_hover_text(tooltip).clicked() {
            // Cycle: System -> Dark -> Light -> System
            config.theme_mode = match config.theme_mode {
                ThemeMode::System => ThemeMode::Dark,
                ThemeMode::Dark => ThemeMode::Light,
                ThemeMode::Light => ThemeMode::System,
            };
            changed = true;
        }
        
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
        
        // NEW: History Button right next to language
        if ui.button(text.history_btn).clicked() {
            *view_mode = ViewMode::History;
        }
    });
    ui.add_space(5.0);

    // Global Settings Button
    let is_global = matches!(view_mode, ViewMode::Global);
    ui.horizontal(|ui| {
        draw_icon_static(ui, Icon::Settings, None);
        if ui.selectable_label(is_global, text.global_settings).clicked() {
            *view_mode = ViewMode::Global;
        }
    });
    
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.presets_section).strong());
    
    let mut preset_idx_to_delete = None;

    for (idx, preset) in config.presets.iter().enumerate() {
        ui.horizontal(|ui| {
            let is_selected = matches!(view_mode, ViewMode::Preset(i) if *i == idx);
            
            let icon_type = if preset.preset_type == "audio" { Icon::Microphone }
            else if preset.preset_type == "video" { Icon::Video }
            else { Icon::Image };
            
            if preset.is_upcoming {
                ui.add_enabled_ui(false, |ui| {
                    ui.horizontal(|ui| {
                        draw_icon_static(ui, icon_type, None);
                        let _ = ui.selectable_label(is_selected, &preset.name);
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    draw_icon_static(ui, icon_type, None);
                    if ui.selectable_label(is_selected, &preset.name).clicked() {
                        *view_mode = ViewMode::Preset(idx);
                    }
                });
                // Delete button (X icon)
                if config.presets.len() > 1 {
                    if icon_button(ui, Icon::Delete).clicked() {
                        preset_idx_to_delete = Some(idx);
                    }
                }
            }
        });
    }
    
    ui.add_space(5.0);
    if ui.button(text.add_preset_btn).clicked() {
        let mut new_preset = Preset::default();
        new_preset.name = format!("Preset {}", config.presets.len() + 1);
        config.presets.push(new_preset);
        *view_mode = ViewMode::Preset(config.presets.len() - 1);
        changed = true;
    }

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
