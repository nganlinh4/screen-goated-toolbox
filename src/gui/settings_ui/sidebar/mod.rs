use super::ViewMode;
use crate::config::{Config, Preset};
use crate::gui::icons::{Icon, draw_icon_static, icon_button_sized};
use crate::gui::locale::LocaleText;
use eframe::egui;

mod localized;
mod profiles;

pub use localized::get_localized_preset_name;

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let mut preset_to_add_type = None;
    let mut preset_idx_to_select: Option<usize> = None;
    let mut preset_idx_to_delete = None;
    let mut preset_idx_to_clone = None;
    let mut preset_idx_to_toggle_favorite = None;
    let mut preset_swap_request = None;

    if profiles::render_profiles(ui, config, view_mode, text) {
        changed = true;
    }

    // Get currently dragging item index from memory (if any)
    let dragging_idx_id = egui::Id::new("sidebar_drag_source");
    let dragging_source_idx: Option<usize> = ui.memory(|mem| mem.data.get_temp(dragging_idx_id));

    let mut image_indices = Vec::new();
    let mut text_indices = Vec::new();
    let mut audio_video_indices = Vec::new();

    for (i, p) in config.presets.iter().enumerate() {
        match p.preset_type.as_str() {
            "image" => image_indices.push(i),
            "text" => text_indices.push(i),
            "audio" | "video" => audio_video_indices.push(i),
            _ => image_indices.push(i),
        }
    }

    // Audio/Video indices are not sorted by type to allow user reordering.
    // They will appear in the order they are defined in config.presets.

    let current_view_mode = *view_mode;
    // Use actual grid width from previous frame for Global Settings position
    thread_local! {
        static GRID_WIDTH: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    }

    // --- Presets Grid ---
    // Use stable ID based on preset count and IDs (not names - those change during typing)
    let preset_hash: u64 = config
        .presets
        .iter()
        .fold(config.presets.len() as u64, |acc, p| {
            acc.wrapping_mul(31).wrapping_add(
                p.id.bytes()
                    .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)),
            )
        });
    let grid_id = egui::Id::new("presets_grid").with(preset_hash);

    let grid_response = egui::Grid::new(grid_id)
        .num_columns(6)
        .spacing([8.0, 4.0])
        .min_col_width(67.0)
        .show(ui, |ui| {
            let theme = crate::gui::theme::AppTheme::from_ui(ui);
            let img_bg = theme.modality_image();
            let txt_bg = theme.modality_text();
            let aud_bg = theme.modality_audio();

            // Preset items, with each add button at the end of its modality list.
            let max_len = image_indices
                .len()
                .max(text_indices.len())
                .max(audio_video_indices.len())
                + 1;
            for i in 0..max_len {
                // Column 1&2: Image
                if let Some(&idx) = image_indices.get(i) {
                    render_preset_item_parts(
                        ui,
                        &config.presets,
                        idx,
                        dragging_source_idx,
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_swap_request,
                        &config.ui_language,
                    );
                } else if i == image_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.add_image_preset_btn,
                        img_bg,
                        "image",
                        &mut preset_to_add_type,
                    );
                } else {
                    ui.label("");
                    ui.label("");
                }

                // Column 3&4: Text
                if let Some(&idx) = text_indices.get(i) {
                    render_preset_item_parts(
                        ui,
                        &config.presets,
                        idx,
                        dragging_source_idx,
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_swap_request,
                        &config.ui_language,
                    );
                } else if i == text_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.add_text_preset_btn,
                        txt_bg,
                        "text",
                        &mut preset_to_add_type,
                    );
                } else {
                    ui.label("");
                    ui.label("");
                }

                // Column 5&6: Audio
                if let Some(&idx) = audio_video_indices.get(i) {
                    render_preset_item_parts(
                        ui,
                        &config.presets,
                        idx,
                        dragging_source_idx,
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_swap_request,
                        &config.ui_language,
                    );
                } else if i == audio_video_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.add_audio_preset_btn,
                        aud_bg,
                        "audio",
                        &mut preset_to_add_type,
                    );
                } else {
                    ui.label("");
                    ui.label("");
                }

                ui.end_row();
            }
        });

    // Update cached grid width for next frame
    GRID_WIDTH.with(|w| w.set(grid_response.response.rect.width()));

    if let Some(idx) = preset_idx_to_select {
        *view_mode = ViewMode::Preset(idx);
    }

    if let Some(idx) = preset_idx_to_toggle_favorite
        && let Some(preset) = config.presets.get_mut(idx)
    {
        preset.is_favorite = !preset.is_favorite;
        changed = true;
        crate::overlay::favorite_bubble::update_favorites_panel();
        crate::overlay::favorite_bubble::trigger_blink_animation();
    }

    if let Some(idx) = preset_idx_to_clone {
        let mut new_preset = config.presets[idx].clone();
        new_preset.id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base_name = if config.presets[idx].id.starts_with("preset_") {
            get_localized_preset_name(&config.presets[idx].id, &config.ui_language)
        } else {
            new_preset.name.clone()
        };
        let mut new_name = format!("{} Copy", base_name);
        let mut counter = 1;
        while config.presets.iter().any(|p| p.name == new_name) {
            new_name = format!("{} Copy {}", base_name, counter);
            counter += 1;
        }
        new_preset.name = new_name;
        new_preset.hotkeys.clear();
        config.presets.push(new_preset);
        *view_mode = ViewMode::Preset(config.presets.len() - 1);
        changed = true;
    }

    if let Some((idx_a, idx_b)) = preset_swap_request {
        // Swap presets
        config.presets.swap(idx_a, idx_b);
        // If currently selecting one of them, update view_mode
        if let ViewMode::Preset(current) = view_mode {
            if *current == idx_a {
                *view_mode = ViewMode::Preset(idx_b);
            } else if *current == idx_b {
                *view_mode = ViewMode::Preset(idx_a);
            }
        }
        changed = true;
    }

    if let Some(type_str) = preset_to_add_type {
        let mut new_preset = Preset::default();
        if type_str == "text" {
            new_preset.preset_type = "text".to_string();
            new_preset.name = format!("Text {}", config.presets.len() + 1);
            new_preset.text_input_mode = "select".to_string();
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "text".to_string();
                block.model = "gemma-4-26b-a4b".to_string();
                block.prompt = "Translate this text.".to_string();
            }
        } else if type_str == "audio" {
            new_preset.preset_type = "audio".to_string();
            new_preset.name = format!("Audio {}", config.presets.len() + 1);
            new_preset.audio_source = "mic".to_string();
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "audio".to_string();
                block.model = "whisper-fast".to_string();
            }
        } else {
            new_preset.name = format!("Image {}", config.presets.len() + 1);
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "image".to_string();
                block.model = crate::model_config::DEFAULT_IMAGE_MODEL_ID.to_string();
                block.prompt = "Extract text from this image.".to_string();
            }
        }
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

fn render_add_preset_button_parts(
    ui: &mut egui::Ui,
    label: &str,
    bg: egui::Color32,
    preset_type: &'static str,
    preset_to_add_type: &mut Option<&'static str>,
) {
    ui.vertical(|ui| {
        ui.add_space(3.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .color(egui::Color32::WHITE)
                        .strong(),
                )
                .fill(bg)
                .corner_radius(12.0),
            )
            .clicked()
        {
            *preset_to_add_type = Some(preset_type);
        }
    });
    ui.label("");
}

#[expect(
    clippy::too_many_arguments,
    reason = "sidebar item rendering keeps per-item actions and drag state explicit"
)]
fn render_preset_item_parts(
    ui: &mut egui::Ui,
    presets: &[Preset],
    idx: usize,
    dragging_source_idx: Option<usize>,
    current_view_mode: &ViewMode,
    preset_idx_to_select: &mut Option<usize>,
    preset_idx_to_delete: &mut Option<usize>,
    preset_idx_to_clone: &mut Option<usize>,
    preset_idx_to_toggle_favorite: &mut Option<usize>,
    preset_swap_request: &mut Option<(usize, usize)>,
    lang: &str,
) {
    let preset = &presets[idx];
    let display_name = if preset.id.starts_with("preset_") {
        get_localized_preset_name(&preset.id, lang)
    } else {
        preset.name.clone()
    };
    let is_selected = matches!(current_view_mode, ViewMode::Preset(i) if *i == idx);
    let has_hotkey = !preset.hotkeys.is_empty();

    let icon_type = match preset.preset_type.as_str() {
        "audio" => {
            if preset.audio_processing_mode == "realtime" {
                Icon::Realtime
            } else if preset.audio_source == "device" {
                Icon::Speaker
            } else {
                Icon::Microphone
            }
        }
        "video" => Icon::Image,
        "text" => {
            if preset.text_input_mode == "select" {
                Icon::TextSelect
            } else {
                Icon::Text
            }
        }
        _ => Icon::Image,
    };

    // --- Column X: Content ---
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        if has_hotkey && !preset.is_upcoming {
            let rect = ui.available_rect_before_wrap();
            let is_dark = ui.visuals().dark_mode;
            let bg_color = if is_dark {
                egui::Color32::from_rgba_unmultiplied(40, 150, 130, 70)
            } else {
                egui::Color32::from_rgb(200, 235, 220)
            };
            ui.painter().rect_filled(rect, 4.0, bg_color);
        }
        if preset.is_upcoming {
            ui.add_enabled_ui(false, |ui| {
                draw_icon_static(ui, icon_type, Some(14.0));
                let _ = ui.selectable_label(is_selected, &display_name);
            });
        } else {
            draw_icon_static(ui, icon_type, Some(14.0));
            // Make the label draggable.
            // SelectableLabel by default captures clicks. We want to also capture drags.
            let label_response = ui.selectable_label(is_selected, &display_name);
            let response = ui.interact(label_response.rect, label_response.id, egui::Sense::drag());

            // Drag interaction on the same rect can consume the first click.
            if label_response.clicked() || response.clicked() {
                *preset_idx_to_select = Some(idx);
            }

            // Drag Source Logic
            let dragging_id = egui::Id::new("sidebar_drag_source");
            if response.drag_started() {
                ui.memory_mut(|mem| mem.data.insert_temp(dragging_id, idx));
            }
            if response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if response.drag_stopped() {
                // Clear state when drag stops
                ui.memory_mut(|mem| mem.data.remove::<usize>(dragging_id));
            }

            // Drop Target Logic
            // If dragging, and we are not the source, and hovered, and released
            if let Some(source_idx) = dragging_source_idx
                && source_idx != idx
                && response.hovered()
                && ui.input(|i| i.pointer.any_released())
            {
                // Check if they are in the same column group
                let source_preset = &presets[source_idx];
                // Target is `preset`

                let get_group = |p: &Preset| -> u8 {
                    match p.preset_type.as_str() {
                        "text" => 1,
                        "audio" | "video" => 2,
                        _ => 0, // Image or default
                    }
                };

                if get_group(source_preset) == get_group(preset) {
                    *preset_swap_request = Some((source_idx, idx));
                }
            }
        }
    });

    // --- Column X+1: Actions ---
    // Use horizontal layout (not right_to_left) to prevent column expansion
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if !preset.is_upcoming {
            // Drag handle removed - label is now draggable

            if icon_button_sized(ui, Icon::CopySmall, 22.0).clicked() {
                *preset_idx_to_clone = Some(idx);
            }
            let star_icon = if preset.is_favorite {
                Icon::StarFilled
            } else {
                Icon::Star
            };
            if icon_button_sized(ui, star_icon, 22.0).clicked() {
                *preset_idx_to_toggle_favorite = Some(idx);
            }
            if presets.len() > 1 && icon_button_sized(ui, Icon::Delete, 22.0).clicked() {
                *preset_idx_to_delete = Some(idx);
            }
        }
    });
}
