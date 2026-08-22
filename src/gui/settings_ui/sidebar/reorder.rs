//! Drag-reordering for the preset lanes.
//!
//! Each modality column is its own sortable list: a preset is lifted onto the
//! cursor, the presets below it close ranks, and a gap marks where it will land.
//! The config is only rewritten when the pointer is released, so nothing shifts
//! under the cursor mid-drag. See [`crate::gui::settings_ui::list_reorder`] for
//! the shared state machine.

use super::row_visuals::{PRESET_CONTENT_MIN_WIDTH, PRESET_ROW_HEIGHT, icon_drag_morph};
use crate::config::Preset;
use crate::gui::icons::{ICON_LG, ICON_SM, Icon, paint_icon};
use crate::gui::settings_ui::list_reorder::{ListReorder, Slot};
use eframe::egui;

const ROW_SETTLE_SECONDS: f32 = 0.14;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Modality {
    Image,
    Text,
    AudioVideo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PresetMoveOutcome {
    pub from: usize,
    pub to: usize,
    pub favorite_indices_changed: bool,
}

fn modality(preset: &Preset) -> Modality {
    match preset.preset_type.as_str() {
        "text" => Modality::Text,
        "audio" | "video" => Modality::AudioVideo,
        _ => Modality::Image,
    }
}

/// What a lane puts in its cell of a given grid row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LaneCell {
    /// The preset at this config index, sitting at `lane_pos` within the lane.
    Preset { idx: usize, lane_pos: usize },
    /// The hole a lifted preset will drop into.
    Gap,
    /// This lane's "add preset" button, which follows its last preset.
    Add,
    /// A shorter lane's filler, keeping the grid rectangular.
    Empty,
}

/// One modality column, with its drag state and this frame's draw plan.
pub(super) struct Lane {
    pub(super) reorder: ListReorder,
    indices: Vec<usize>,
    plan: Vec<Slot>,
}

impl Lane {
    pub(super) fn load(ui: &egui::Ui, key: &'static str, indices: Vec<usize>) -> Self {
        let mut reorder = ListReorder::load(ui, key);
        reorder.track(ui, indices.len());
        let plan = reorder.plan(indices.len());
        Self {
            reorder,
            indices,
            plan,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn is_lifting(&self) -> bool {
        self.reorder.is_lifting()
    }

    pub(super) fn cell(&self, row: usize) -> LaneCell {
        match self.plan.get(row) {
            Some(Slot::Step(lane_pos)) => LaneCell::Preset {
                idx: self.indices[*lane_pos],
                lane_pos: *lane_pos,
            },
            Some(Slot::Gap) => LaneCell::Gap,
            None if row == self.indices.len() => LaneCell::Add,
            None => LaneCell::Empty,
        }
    }

    /// Track the cursor, and on release report the config indices to move.
    pub(super) fn settle(&mut self, ui: &egui::Ui) -> Option<(usize, usize)> {
        let (from, to) = self.reorder.settle(ui)?;
        Some((*self.indices.get(from)?, *self.indices.get(to)?))
    }

    pub(super) fn store(self, ui: &egui::Ui) {
        self.reorder.store(ui);
    }
}

/// How far a row still is from the slot it now occupies.
///
/// Armed only while a preset is lifted: the sidebar re-lays out whenever the
/// window resizes, and animating those corrections would slide the whole list
/// around for reasons that have nothing to do with reordering.
pub(super) fn animated_row_offset(ui: &egui::Ui, preset_id: &str, lifting: bool) -> egui::Vec2 {
    let target_y = ui.next_widget_position().y;
    let animation_id = egui::Id::new("preset-row-position").with(preset_id);
    let seconds = if lifting { ROW_SETTLE_SECONDS } else { 0.0 };
    let animated_y = ui
        .ctx()
        .animate_value_with_time(animation_id, target_y, seconds);
    egui::vec2(0.0, animated_y - target_y)
}

/// Paint the preset's type icon and its drag handle into a single `ICON_LG` slot,
/// cross-faded by the pointer's proximity to that slot (0 = pure type icon,
/// 1 = pure drag handle). The slot is what lifts the row, so a preset can be
/// grabbed the moment the handle reads.
pub(super) fn render_drag_handle(
    ui: &mut egui::Ui,
    preset: &Preset,
    type_icon: Icon,
) -> egui::Response {
    let drag_id = egui::Id::new("preset-drag-handle").with(&preset.id);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ICON_LG, ICON_LG), egui::Sense::hover());
    let response = ui.interact(rect, drag_id, egui::Sense::drag());
    let morph = if response.dragged() {
        1.0
    } else {
        icon_drag_morph(ui, rect, &preset.id)
    };

    if response.hovered() {
        ui.painter().rect_filled(
            rect.shrink(1.0),
            3.0,
            ui.visuals().widgets.hovered.bg_fill.gamma_multiply(morph),
        );
    }
    paint_icon(
        ui.painter(),
        rect,
        type_icon,
        ui.visuals().text_color().gamma_multiply(1.0 - morph),
    );
    let handle_color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    // The handle keeps its own smaller footprint, centered in the shared slot,
    // so the swap reads as a fade rather than a size pop.
    paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::vec2(ICON_SM, ICON_SM)),
        Icon::DragIndicator,
        handle_color.gamma_multiply(morph),
    );
    response.on_hover_cursor(egui::CursorIcon::Grab)
}

/// The hole the carried preset will drop into: an outlined slot the same size as
/// a row, plus the empty actions cell that keeps the grid columns aligned.
pub(super) fn render_gap_cells(ui: &mut egui::Ui, lane: &Lane) {
    let fallback = egui::vec2(PRESET_CONTENT_MIN_WIDTH, PRESET_ROW_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(lane.reorder.slot_size(fallback), egui::Sense::hover());
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        4.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    ui.label("");
}

/// The carried preset itself: a copy that tracks the cursor above the sidebar,
/// lifted off the surface with a shadow. Inert by design — the live row controls
/// stay in the list, which keeps their ids unique.
pub(super) fn draw_lifted_preset(ui: &mut egui::Ui, lane: &Lane, presets: &[Preset], lang: &str) {
    let Some(floating) = lane.reorder.floating(ui) else {
        return;
    };
    let Some(preset) = lane
        .indices
        .get(floating.from)
        .and_then(|idx| presets.get(*idx))
    else {
        return;
    };
    let theme = crate::gui::theme::AppTheme::from_ui(ui);
    let icon = super::preset_type_icon(preset);
    let name = super::preset_display_name(preset, lang);

    egui::Area::new(egui::Id::new(("preset-lifted", &preset.id)))
        // Tooltip order keeps the copy above the settings panel and any modal
        // surface, the same layer egui's own drag preview uses.
        .order(egui::Order::Tooltip)
        .fixed_pos(floating.origin)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            ui.set_width(floating.size.x);
            egui::Frame::new()
                .fill(theme.card_bg())
                .stroke(theme.card_stroke())
                .corner_radius(egui::CornerRadius::same(6))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: theme.scrim_color(),
                })
                .inner_margin(egui::Margin::symmetric(
                    crate::gui::theme::space::HAIR,
                    crate::gui::theme::space::MICRO,
                ))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(PRESET_ROW_HEIGHT);
                        crate::gui::icons::draw_icon_static(ui, icon, Some(ICON_LG));
                        ui.label(name);
                    });
                });
        });
}

/// Move a preset within its own modality, reporting what the caller must remap.
pub(super) fn apply_move(
    presets: &mut Vec<Preset>,
    from: usize,
    to: usize,
) -> Option<PresetMoveOutcome> {
    if from == to || from >= presets.len() || to >= presets.len() {
        return None;
    }
    if modality(&presets[from]) != modality(&presets[to]) {
        return None;
    }

    let affected = from.min(to)..=from.max(to);
    let favorite_indices_changed = presets[affected].iter().any(|preset| preset.is_favorite);
    let preset = presets.remove(from);
    presets.insert(to, preset);
    Some(PresetMoveOutcome {
        from,
        to,
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
    fn move_preserves_relative_order_and_reports_favorite_churn() {
        let mut presets = vec![
            preset("a", "image", false),
            preset("b", "image", true),
            preset("c", "image", false),
        ];
        let outcome = apply_move(&mut presets, 0, 2).unwrap();

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
    fn move_rejects_cross_modality_targets() {
        let mut presets = vec![
            preset("image", "image", false),
            preset("text", "text", false),
        ];
        assert!(apply_move(&mut presets, 0, 1).is_none());
    }

    #[test]
    fn a_lane_maps_grid_rows_to_presets_gaps_and_its_add_button() {
        let ctx = egui::Context::default();
        let mut cells = Vec::new();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            // Config indices 1, 4 and 7 belong to this lane; the rest are other
            // modalities interleaved between them.
            let lane = Lane::load(ui, "test-lane", vec![1, 4, 7]);
            cells = (0..5).map(|row| lane.cell(row)).collect();
        });

        assert_eq!(
            cells,
            vec![
                LaneCell::Preset {
                    idx: 1,
                    lane_pos: 0
                },
                LaneCell::Preset {
                    idx: 4,
                    lane_pos: 1
                },
                LaneCell::Preset {
                    idx: 7,
                    lane_pos: 2
                },
                LaneCell::Add,
                LaneCell::Empty,
            ]
        );
    }
}
