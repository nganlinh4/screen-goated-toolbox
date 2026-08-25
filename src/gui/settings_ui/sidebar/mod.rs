use super::ViewMode;
use crate::config::{Config, Preset};
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use eframe::egui;
use row_visuals::{
    PRESET_CONTENT_GAP, PRESET_CONTENT_MIN_WIDTH, PRESET_GRID_COLUMN_GAP, PRESET_GRID_TOP_GAP,
    PRESET_OVERLAY_PITCH, overlay_bridge, overlay_icon_button, paint_overlay_bridge,
    preset_action_id, preset_row_band, proximity_icon_button, proximity_opacity,
    trailing_overlay_slot,
};

#[cfg(test)]
mod layout_tests;
mod localized;
mod profiles;
mod reorder;
mod row_visuals;

pub use localized::get_localized_preset_name;

thread_local! {
    static GRID_WIDTH: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
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
    ui.add_space(PRESET_GRID_TOP_GAP);

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

    let mut image_lane = reorder::Lane::load(ui, "preset-lane-image", image_indices);
    let mut text_lane = reorder::Lane::load(ui, "preset-lane-text", text_indices);
    let mut audio_lane = reorder::Lane::load(ui, "preset-lane-audio", audio_video_indices);
    let lifting = image_lane.is_lifting() || text_lane.is_lifting() || audio_lane.is_lifting();

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
            let max_len = image_lane.len().max(text_lane.len()).max(audio_lane.len()) + 1;
            for i in 0..max_len {
                // Column 1&2: Image
                match image_lane.cell(i) {
                    reorder::LaneCell::Preset { idx, lane_pos } => render_preset_item_parts(
                        ui,
                        PresetRow {
                            presets: &config.presets,
                            idx,
                            lane_pos,
                            lifting,
                        },
                        &current_view_mode,
                        &mut RowActions {
                            select: &mut preset_idx_to_select,
                            delete: &mut preset_idx_to_delete,
                            clone: &mut preset_idx_to_clone,
                            favorite: &mut preset_idx_to_toggle_favorite,
                        },
                        &mut image_lane,
                        &config.ui_language,
                    ),
                    reorder::LaneCell::Gap => reorder::render_gap_cells(ui, &image_lane),
                    reorder::LaneCell::Add => render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_image_preset_btn,
                        img_bg,
                        "image",
                        &mut preset_to_add_type,
                    ),
                    reorder::LaneCell::Empty => {
                        ui.label("");
                        ui.label("");
                    }
                }

                // Column 3&4: Text
                match text_lane.cell(i) {
                    reorder::LaneCell::Preset { idx, lane_pos } => render_preset_item_parts(
                        ui,
                        PresetRow {
                            presets: &config.presets,
                            idx,
                            lane_pos,
                            lifting,
                        },
                        &current_view_mode,
                        &mut RowActions {
                            select: &mut preset_idx_to_select,
                            delete: &mut preset_idx_to_delete,
                            clone: &mut preset_idx_to_clone,
                            favorite: &mut preset_idx_to_toggle_favorite,
                        },
                        &mut text_lane,
                        &config.ui_language,
                    ),
                    reorder::LaneCell::Gap => reorder::render_gap_cells(ui, &text_lane),
                    reorder::LaneCell::Add => render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_text_preset_btn,
                        txt_bg,
                        "text",
                        &mut preset_to_add_type,
                    ),
                    reorder::LaneCell::Empty => {
                        ui.label("");
                        ui.label("");
                    }
                }

                // Column 5&6: Audio
                match audio_lane.cell(i) {
                    reorder::LaneCell::Preset { idx, lane_pos } => render_preset_item_parts(
                        ui,
                        PresetRow {
                            presets: &config.presets,
                            idx,
                            lane_pos,
                            lifting,
                        },
                        &current_view_mode,
                        &mut RowActions {
                            select: &mut preset_idx_to_select,
                            delete: &mut preset_idx_to_delete,
                            clone: &mut preset_idx_to_clone,
                            favorite: &mut preset_idx_to_toggle_favorite,
                        },
                        &mut audio_lane,
                        &config.ui_language,
                    ),
                    reorder::LaneCell::Gap => reorder::render_gap_cells(ui, &audio_lane),
                    reorder::LaneCell::Add => render_add_preset_button_parts(
                        ui,
                        text.preset_editor.add_audio_preset_btn,
                        aud_bg,
                        "audio",
                        &mut preset_to_add_type,
                    ),
                    reorder::LaneCell::Empty => {
                        ui.label("");
                        ui.label("");
                    }
                }

                ui.end_row();
            }
        });

    GRID_WIDTH.with(|width| width.set(grid_response.response.rect.width()));

    for lane in [&image_lane, &text_lane, &audio_lane] {
        reorder::draw_lifted_preset(ui, lane, &config.presets, &config.ui_language);
    }
    for lane in [&mut image_lane, &mut text_lane, &mut audio_lane] {
        if let Some(landed) = lane.settle(ui) {
            preset_move_request = Some(landed);
        }
    }
    image_lane.store(ui);
    text_lane.store(ui);
    audio_lane.store(ui);

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

    if let Some((from, to)) = preset_move_request
        && let Some(outcome) = reorder::apply_move(&mut config.presets, from, to)
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

/// Icon standing for a preset's modality and input mode.
fn preset_type_icon(preset: &Preset) -> Icon {
    match preset.preset_type.as_str() {
        "audio" => {
            if preset.audio_source == "device" {
                Icon::Speaker
            } else {
                Icon::Microphone
            }
        }
        "text" => {
            if preset.text_input_mode == "select" {
                Icon::TextSelect
            } else {
                Icon::Keyboard
            }
        }
        _ => Icon::Image,
    }
}

fn render_add_preset_button_parts(
    ui: &mut egui::Ui,
    label: &str,
    bg: egui::Color32,
    preset_type: &'static str,
    preset_to_add_type: &mut Option<&'static str>,
) {
    // The add button shares the preset rows' band. Left to itself it is taller
    // than a row, which stretches the grid row it sits in and opens a gap between
    // a lane's last preset and its button that the other lanes do not have.
    preset_row_band(ui, PRESET_CONTENT_MIN_WIDTH, |ui, _| {
        ui.spacing_mut().button_padding.y = 3.0;
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

/// The preset a row draws, and where it sits in its lane.
struct PresetRow<'a> {
    presets: &'a [Preset],
    idx: usize,
    lane_pos: usize,
    /// Whether any lane is mid-drag, which arms the row-settle animation.
    lifting: bool,
}

/// Per-row actions the caller applies once the grid is done.
struct RowActions<'a> {
    select: &'a mut Option<usize>,
    delete: &'a mut Option<usize>,
    clone: &'a mut Option<usize>,
    favorite: &'a mut Option<usize>,
}

/// The label a preset shows: built-ins are localized, user presets keep the name
/// they were given.
fn preset_display_name(preset: &Preset, lang: &str) -> String {
    if preset.id.starts_with("preset_") {
        get_localized_preset_name(&preset.id, lang)
    } else {
        preset.name.clone()
    }
}

fn render_preset_item_parts(
    ui: &mut egui::Ui,
    row: PresetRow<'_>,
    current_view_mode: &ViewMode,
    actions: &mut RowActions<'_>,
    lane: &mut reorder::Lane,
    lang: &str,
) {
    let PresetRow {
        presets,
        idx,
        lane_pos,
        lifting,
    } = row;
    let preset_idx_to_select = &mut *actions.select;
    let preset_idx_to_delete = &mut *actions.delete;
    let preset_idx_to_clone = &mut *actions.clone;
    let preset_idx_to_toggle_favorite = &mut *actions.favorite;
    let preset = &presets[idx];
    let display_name = preset_display_name(preset, lang);
    let is_selected = matches!(current_view_mode, ViewMode::Preset(i) if *i == idx);
    let has_hotkey = !preset.hotkeys.is_empty();

    let icon_type = preset_type_icon(preset);

    let row_offset = reorder::animated_row_offset(ui, &preset.id, lifting);
    let transform = egui::emath::TSTransform::from_translation(row_offset);

    // --- Column X: Content ---
    let mut lift_request = None;
    let content_response = ui.with_visual_transform(transform, |ui| {
        // Copy and delete overlay this cell's tail, so it must stay wide enough
        // for them to clear the type icon even with a short label.
        preset_row_band(ui, PRESET_CONTENT_MIN_WIDTH, |ui, cell_rect| {
            ui.spacing_mut().item_spacing.x = PRESET_CONTENT_GAP;
            let chip = (has_hotkey && !preset.is_upcoming)
                .then(|| crate::gui::theme::AppTheme::from_ui(ui).hotkey_chip_bg());
            if let Some(chip) = chip {
                ui.painter().rect_filled(cell_rect, 4.0, chip);
            }
            if preset.is_upcoming {
                ui.add_enabled_ui(false, |ui| {
                    draw_icon_static(ui, icon_type, Some(crate::gui::icons::ICON_LG));
                    let _ = ui.selectable_label(is_selected, &display_name);
                });
            } else {
                // The drag handle lives in the type icon's slot and cross-fades in
                // on approach, so reordering costs the row no horizontal space.
                let handle = reorder::render_drag_handle(ui, preset, icon_type);
                if handle.drag_started() {
                    lift_request = Some(cell_rect);
                }
                if ui.selectable_label(is_selected, &display_name).clicked() {
                    *preset_idx_to_select = Some(idx);
                }
                // Copy and delete float over the label's tail rather than claiming
                // columns of their own. They must stay inside this cell: in a Grid
                // every allocation is a new column, and they deliberately make none.
                // Painted last so they mask the text they cover.
                let can_delete = presets.len() > 1;
                let mut slot = trailing_overlay_slot(cell_rect.right(), handle.rect.center().y);
                let bridge = overlay_bridge(slot, 1 + usize::from(can_delete));
                let opacity =
                    proximity_opacity(ui.input(|input| input.pointer.hover_pos()), bridge);
                paint_overlay_bridge(ui, bridge, chip, opacity);
                if can_delete {
                    if overlay_icon_button(
                        ui,
                        slot,
                        Icon::Delete,
                        preset_action_id(&preset.id, "delete"),
                        opacity,
                    )
                    .clicked()
                    {
                        *preset_idx_to_delete = Some(idx);
                    }
                    slot = slot.translate(egui::vec2(-PRESET_OVERLAY_PITCH, 0.0));
                }
                if overlay_icon_button(
                    ui,
                    slot,
                    Icon::CopySmall,
                    preset_action_id(&preset.id, "copy"),
                    opacity,
                )
                .clicked()
                {
                    *preset_idx_to_clone = Some(idx);
                }
            }
        });
    });
    lane.reorder.note_slot(content_response.response.rect);
    if let Some(row_rect) = lift_request {
        lane.reorder.lift(ui, lane_pos, row_rect);
    }

    // --- Column X+1: Actions ---
    // Only the favorite star still claims layout space here: it stays pinned and
    // fully opaque while favorited, and is the target of the favorites blink.
    ui.with_visual_transform(transform, |ui| {
        preset_row_band(ui, crate::gui::icons::ICON_LG, |ui, _| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if !preset.is_upcoming {
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
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
