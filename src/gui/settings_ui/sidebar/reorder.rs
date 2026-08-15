use crate::config::Preset;
use crate::gui::icons::{ICON_SM, Icon, paint_icon};
use eframe::egui;

const ROW_SETTLE_SECONDS: f32 = 0.14;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresetLane {
    Image,
    Text,
    AudioVideo,
}

#[derive(Clone, Debug)]
struct PresetDragPayload {
    preset_id: String,
    lane: PresetLane,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PresetMoveRequest {
    preset_id: String,
    target_idx: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PresetMoveOutcome {
    pub from: usize,
    pub to: usize,
    pub favorite_indices_changed: bool,
}

fn lane(preset: &Preset) -> PresetLane {
    match preset.preset_type.as_str() {
        "text" => PresetLane::Text,
        "audio" | "video" => PresetLane::AudioVideo,
        _ => PresetLane::Image,
    }
}

pub(super) fn animated_row_offset(ui: &egui::Ui, preset_id: &str) -> egui::Vec2 {
    let target_y = ui.next_widget_position().y;
    let animation_id = egui::Id::new("preset-row-position").with(preset_id);
    let animated_y = ui
        .ctx()
        .animate_value_with_time(animation_id, target_y, ROW_SETTLE_SECONDS);
    egui::vec2(0.0, animated_y - target_y)
}

pub(super) fn render_drag_handle(
    ui: &mut egui::Ui,
    preset: &Preset,
    proximity_opacity: f32,
) -> egui::Response {
    let payload = PresetDragPayload {
        preset_id: preset.id.clone(),
        lane: lane(preset),
    };
    let drag_id = egui::Id::new("preset-drag-handle").with(&preset.id);
    let opacity = if ui.ctx().is_being_dragged(drag_id) {
        1.0
    } else {
        proximity_opacity.clamp(0.0, 1.0)
    };
    ui.dnd_drag_source(drag_id, payload, |ui| {
        let size = egui::vec2(ICON_SM, ICON_SM);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        if response.hovered() {
            ui.painter().rect_filled(
                rect.shrink(1.0),
                3.0,
                ui.visuals().widgets.hovered.bg_fill.gamma_multiply(opacity),
            );
        }
        let color = if response.hovered() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        paint_icon(
            ui.painter(),
            rect,
            Icon::DragIndicator,
            color.gamma_multiply(opacity),
        );
    })
    .response
}

pub(super) fn move_request_for_row(
    ui: &egui::Ui,
    presets: &[Preset],
    target_idx: usize,
    row_rect: egui::Rect,
) -> Option<PresetMoveRequest> {
    let payload = egui::DragAndDrop::payload::<PresetDragPayload>(ui.ctx())?;
    let target = presets.get(target_idx)?;
    if payload.preset_id == target.id || payload.lane != lane(target) {
        return None;
    }

    let source_idx = presets
        .iter()
        .position(|preset| preset.id == payload.preset_id)?;
    let pointer = ui.ctx().pointer_interact_pos()?;
    if !row_rect.expand2(egui::vec2(4.0, 2.0)).contains(pointer) {
        return None;
    }

    let crossed_insertion_edge = if source_idx < target_idx {
        pointer.y >= row_rect.center().y
    } else {
        pointer.y <= row_rect.center().y
    };
    if !crossed_insertion_edge {
        return None;
    }

    Some(PresetMoveRequest {
        preset_id: payload.preset_id.clone(),
        target_idx,
    })
}

pub(super) fn apply_move(
    presets: &mut Vec<Preset>,
    request: PresetMoveRequest,
) -> Option<PresetMoveOutcome> {
    let source_idx = presets
        .iter()
        .position(|preset| preset.id == request.preset_id)?;
    let target_idx = request.target_idx;
    if source_idx == target_idx || target_idx >= presets.len() {
        return None;
    }
    if lane(&presets[source_idx]) != lane(&presets[target_idx]) {
        return None;
    }

    let affected = source_idx.min(target_idx)..=source_idx.max(target_idx);
    let favorite_indices_changed = presets[affected].iter().any(|preset| preset.is_favorite);
    let preset = presets.remove(source_idx);
    presets.insert(target_idx, preset);
    Some(PresetMoveOutcome {
        from: source_idx,
        to: target_idx,
        favorite_indices_changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, preset_type: &str, favorite: bool) -> Preset {
        Preset {
            id: id.to_owned(),
            preset_type: preset_type.to_owned(),
            is_favorite: favorite,
            ..Preset::default()
        }
    }

    #[test]
    fn move_uses_stable_identity_and_preserves_relative_order() {
        let mut presets = vec![
            preset("a", "image", false),
            preset("b", "image", true),
            preset("c", "image", false),
        ];
        let outcome = apply_move(
            &mut presets,
            PresetMoveRequest {
                preset_id: "a".to_owned(),
                target_idx: 2,
            },
        )
        .unwrap();

        assert_eq!(
            presets
                .iter()
                .map(|preset| preset.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        assert_eq!(outcome.from, 0);
        assert_eq!(outcome.to, 2);
        assert!(outcome.favorite_indices_changed);
    }

    #[test]
    fn move_rejects_cross_lane_targets() {
        let mut presets = vec![
            preset("image", "image", false),
            preset("text", "text", false),
        ];
        assert!(
            apply_move(
                &mut presets,
                PresetMoveRequest {
                    preset_id: "image".to_owned(),
                    target_idx: 1,
                },
            )
            .is_none()
        );
    }
}
