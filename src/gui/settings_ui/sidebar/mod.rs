use super::ViewMode;
use crate::config::{Config, Preset};
use crate::gui::icons::{Icon, draw_icon_static, icon_button_sized_with_opacity};
use crate::gui::locale::LocaleText;
use eframe::egui;

mod localized;
mod profiles;
mod reorder;

pub use localized::get_localized_preset_name;

thread_local! {
    static GRID_WIDTH: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

const PRESET_ACTION_REVEAL_RADIUS: f32 = 96.0;
const PRESET_GRID_COLUMN_GAP: f32 = 3.0;
const PRESET_CONTENT_GAP: f32 = 1.0;

fn proximity_opacity(pointer: Option<egui::Pos2>, rect: egui::Rect) -> f32 {
    let Some(pointer) = pointer else {
        return 0.0;
    };
    let dx = if pointer.x < rect.left() {
        rect.left() - pointer.x
    } else if pointer.x > rect.right() {
        pointer.x - rect.right()
    } else {
        0.0
    };
    let dy = if pointer.y < rect.top() {
        rect.top() - pointer.y
    } else if pointer.y > rect.bottom() {
        pointer.y - rect.bottom()
    } else {
        0.0
    };
    (1.0 - dx.hypot(dy) / PRESET_ACTION_REVEAL_RADIUS).clamp(0.0, 1.0)
}

fn proximity_icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    always_visible: bool,
) -> egui::Response {
    let opacity = if always_visible {
        1.0
    } else {
        next_widget_proximity_opacity(ui, size)
    };
    icon_button_sized_with_opacity(ui, icon, size, opacity)
}

fn next_widget_proximity_opacity(ui: &egui::Ui, size: f32) -> f32 {
    let candidate_rect =
        egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(size, size));
    proximity_opacity(ui.input(|input| input.pointer.hover_pos()), candidate_rect)
}

pub(crate) fn cached_grid_width() -> f32 {
    GRID_WIDTH.with(|width| width.get())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetIndexMove {
    pub from: usize,
    pub to: usize,
}

impl PresetIndexMove {
    pub fn remap(self, index: usize) -> usize {
        if index == self.from {
            self.to
        } else if self.from < self.to && (self.from + 1..=self.to).contains(&index) {
            index - 1
        } else if self.to < self.from && (self.to..self.from).contains(&index) {
            index + 1
        } else {
            index
        }
    }
}

#[derive(Default)]
pub struct SidebarRenderResponse {
    pub changed: bool,
    pub refresh_favorites: bool,
    pub blink_favorite: bool,
    pub preset_index_move: Option<PresetIndexMove>,
}

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> SidebarRenderResponse {
    let mut response = SidebarRenderResponse::default();
    let mut preset_to_add_type = None;
    let mut preset_idx_to_select: Option<usize> = None;
    let mut preset_idx_to_delete = None;
    let mut preset_idx_to_clone = None;
    let mut preset_idx_to_toggle_favorite = None;
    let mut preset_move_request = None;

    let profile_response = profiles::render_profiles(ui, config, view_mode, text);
    response.changed |= profile_response.changed;
    response.refresh_favorites |= profile_response.presets_changed;

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

    // Preset order changes continuously during a drag. The Grid identity must
    // remain stable so egui retains its measured six-column layout between moves.
    let grid_response = egui::Grid::new("presets_grid")
        .num_columns(6)
        .spacing([PRESET_GRID_COLUMN_GAP, 4.0])
        .min_col_width(0.0)
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
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_move_request,
                        &config.ui_language,
                    );
                } else if i == image_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_image_preset_btn,
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
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_move_request,
                        &config.ui_language,
                    );
                } else if i == text_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_text_preset_btn,
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
                        &current_view_mode,
                        &mut preset_idx_to_select,
                        &mut preset_idx_to_delete,
                        &mut preset_idx_to_clone,
                        &mut preset_idx_to_toggle_favorite,
                        &mut preset_move_request,
                        &config.ui_language,
                    );
                } else if i == audio_video_indices.len() {
                    render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_audio_preset_btn,
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

    GRID_WIDTH.with(|width| width.set(grid_response.response.rect.width()));

    if let Some(idx) = preset_idx_to_select {
        *view_mode = ViewMode::Preset(idx);
    }

    if let Some(idx) = preset_idx_to_toggle_favorite
        && let Some(preset) = config.presets.get_mut(idx)
    {
        preset.is_favorite = !preset.is_favorite;
        response.changed = true;
        response.refresh_favorites = true;
        response.blink_favorite = true;
    }

    if let Some(idx) = preset_idx_to_clone {
        let mut new_preset = config.presets[idx].clone();
        let clone_is_favorite = new_preset.is_favorite;
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
        response.changed = true;
        response.refresh_favorites |= clone_is_favorite;
    }

    if let Some(move_request) = preset_move_request
        && let Some(outcome) = reorder::apply_move(&mut config.presets, move_request)
    {
        let index_move = PresetIndexMove {
            from: outcome.from,
            to: outcome.to,
        };
        if let ViewMode::Preset(current) = view_mode {
            *current = index_move.remap(*current);
        }
        response.changed = true;
        response.refresh_favorites |= outcome.favorite_indices_changed;
        response.preset_index_move = Some(index_move);
    }

    if let Some(type_str) = preset_to_add_type {
        let mut new_preset = Preset::default();
        if type_str == "text" {
            new_preset.preset_type = "text".to_string();
            new_preset.name = format!("Text {}", config.presets.len() + 1);
            new_preset.text_input_mode = "select".to_string();
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "text".to_string();
                block.model = crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string();
                block.prompt = "Translate this text.".to_string();
            }
        } else if type_str == "audio" {
            new_preset.preset_type = "audio".to_string();
            new_preset.name = format!("Audio {}", config.presets.len() + 1);
            new_preset.audio_source = "mic".to_string();
            if let Some(block) = new_preset.blocks.first_mut() {
                block.block_type = "audio".to_string();
                block.model = crate::model_config::PRESET_AUDIO_TRANSCRIBE_MODEL_ID.to_string();
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
        response.changed = true;
    }

    if let Some(idx) = preset_idx_to_delete {
        let favorite_indices_changed = config.presets[idx..]
            .iter()
            .any(|preset| preset.is_favorite);
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
        response.changed = true;
        response.refresh_favorites |= favorite_indices_changed;
    }

    response
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
    current_view_mode: &ViewMode,
    preset_idx_to_select: &mut Option<usize>,
    preset_idx_to_delete: &mut Option<usize>,
    preset_idx_to_clone: &mut Option<usize>,
    preset_idx_to_toggle_favorite: &mut Option<usize>,
    preset_move_request: &mut Option<reorder::PresetMoveRequest>,
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

    let icon_type = if preset.id == "preset_realtime_audio_translate" {
        Icon::Rtt
    } else {
        match preset.preset_type.as_str() {
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
                    Icon::Keyboard
                }
            }
            _ => Icon::Image,
        }
    };

    let row_offset = reorder::animated_row_offset(ui, &preset.id);
    let transform = egui::emath::TSTransform::from_translation(row_offset);

    // --- Column X: Content ---
    let content_response = ui.with_visual_transform(transform, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            ui.spacing_mut().item_spacing.x = PRESET_CONTENT_GAP;
            if has_hotkey && !preset.is_upcoming {
                let rect = ui.available_rect_before_wrap();
                let bg_color = crate::gui::theme::AppTheme::from_ui(ui).hotkey_chip_bg();
                ui.painter().rect_filled(rect, 4.0, bg_color);
            }
            if preset.is_upcoming {
                ui.add_enabled_ui(false, |ui| {
                    draw_icon_static(ui, icon_type, Some(crate::gui::icons::ICON_LG));
                    let _ = ui.selectable_label(is_selected, &display_name);
                });
            } else {
                draw_icon_static(ui, icon_type, Some(crate::gui::icons::ICON_LG));
                let drag_opacity = next_widget_proximity_opacity(ui, crate::gui::icons::ICON_SM);
                reorder::render_drag_handle(ui, preset, drag_opacity);
                if ui.selectable_label(is_selected, &display_name).clicked() {
                    *preset_idx_to_select = Some(idx);
                }
            }
        });
    });
    if preset_move_request.is_none() {
        *preset_move_request =
            reorder::move_request_for_row(ui, presets, idx, content_response.response.rect);
    }

    // --- Column X+1: Actions ---
    // Use horizontal layout (not right_to_left) to prevent column expansion
    ui.with_visual_transform(transform, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if !preset.is_upcoming {
                if proximity_icon_button(ui, Icon::CopySmall, crate::gui::icons::ICON_SM, false)
                    .clicked()
                {
                    *preset_idx_to_clone = Some(idx);
                }
                let star_icon = if preset.is_favorite {
                    Icon::StarFilled
                } else {
                    Icon::Star
                };
                if proximity_icon_button(
                    ui,
                    star_icon,
                    crate::gui::icons::ICON_LG,
                    preset.is_favorite,
                )
                .clicked()
                {
                    *preset_idx_to_toggle_favorite = Some(idx);
                }
                if presets.len() > 1
                    && proximity_icon_button(ui, Icon::Delete, crate::gui::icons::ICON_SM, false)
                        .clicked()
                {
                    *preset_idx_to_delete = Some(idx);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_action_opacity_tracks_distance_and_hides_without_a_pointer() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(18.0, 18.0));

        assert_eq!(proximity_opacity(None, rect), 0.0);
        assert_eq!(proximity_opacity(Some(rect.center()), rect), 1.0);
        assert_eq!(
            proximity_opacity(
                Some(egui::pos2(
                    rect.right() + PRESET_ACTION_REVEAL_RADIUS,
                    rect.center().y
                )),
                rect,
            ),
            0.0
        );
        let near = proximity_opacity(Some(egui::pos2(rect.right() + 12.0, rect.center().y)), rect);
        let far = proximity_opacity(Some(egui::pos2(rect.right() + 48.0, rect.center().y)), rect);
        assert!(near > far && far > 0.0);
    }

    #[test]
    fn preset_index_move_remaps_source_displaced_items_and_unaffected_items() {
        let downward = PresetIndexMove { from: 0, to: 3 };
        assert_eq!(downward.remap(0), 3);
        assert_eq!(downward.remap(2), 1);
        assert_eq!(downward.remap(5), 5);

        let upward = PresetIndexMove { from: 3, to: 0 };
        assert_eq!(upward.remap(3), 0);
        assert_eq!(upward.remap(1), 2);
    }
}
