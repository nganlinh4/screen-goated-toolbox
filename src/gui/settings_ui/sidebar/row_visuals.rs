//! Row-level visuals for the preset grid: the proximity field that reveals a
//! row's controls, the one-row layout band every lane shares, and the trailing
//! actions that float over a label instead of claiming a column.

use crate::gui::icons::{Icon, icon_button_sized_with_opacity, paint_icon};
use eframe::egui;

const PRESET_ACTION_REVEAL_RADIUS: f32 = 96.0;
pub(super) const PRESET_GRID_COLUMN_GAP: f32 = 3.0;
pub(super) const PRESET_CONTENT_GAP: f32 = 1.0;
/// Reach of the type-icon → drag-handle reveal. Generous on purpose: the handle
/// should start surfacing while the pointer is still crossing the panel.
const PRESET_DRAG_MORPH_RADIUS: f32 = 170.0;
const PRESET_DRAG_MORPH_SECONDS: f32 = 0.09;
pub(super) const PRESET_ROW_HEIGHT: f32 = 22.0;
/// Separates the first preset row from the profiles bar above it.
pub(super) const PRESET_GRID_TOP_GAP: f32 = 6.0;
const PRESET_OVERLAY_BUTTON: f32 = 18.0;
/// Trailing actions step by the same pitch the grid puts between the content cell
/// and the star, so copy → delete → star are evenly spaced despite the star
/// living in its own column.
pub(super) const PRESET_OVERLAY_PITCH: f32 = PRESET_OVERLAY_BUTTON + PRESET_GRID_COLUMN_GAP;
/// Room for the type icon, a readable stretch of label, and the two overlay
/// actions that sit on its tail.
pub(super) const PRESET_CONTENT_MIN_WIDTH: f32 = 96.0;

/// Radial falloff from the pointer to a widget's rect: 1 inside it, easing to 0
/// at `radius` in any direction.
fn proximity_opacity_within(pointer: Option<egui::Pos2>, rect: egui::Rect, radius: f32) -> f32 {
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
    (1.0 - dx.hypot(dy) / radius).clamp(0.0, 1.0)
}

pub(super) fn proximity_opacity(pointer: Option<egui::Pos2>, rect: egui::Rect) -> f32 {
    proximity_opacity_within(pointer, rect, PRESET_ACTION_REVEAL_RADIUS)
}

pub(super) fn proximity_icon_button(
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

/// How far the type icon has morphed into the drag handle: the same radial
/// proximity the row actions use, then smoothed over time so a pointer arriving
/// from off-screen fades in. Takes the slot's *allocated* rect, not a predicted
/// one, so the field is centred exactly where the icon is painted.
pub(super) fn icon_drag_morph(ui: &egui::Ui, icon_rect: egui::Rect, preset_id: &str) -> f32 {
    let target = proximity_opacity_within(
        ui.input(|input| input.pointer.hover_pos()),
        icon_rect,
        PRESET_DRAG_MORPH_RADIUS,
    );
    let animation_id = egui::Id::new("preset-drag-morph").with(preset_id);
    ui.ctx()
        .animate_value_with_time(animation_id, target, PRESET_DRAG_MORPH_SECONDS)
}

/// Lay a grid cell's contents out in a band exactly one row tall.
///
/// A grid cell's available height is whatever is left below it in the grid, which
/// differs from lane to lane; a plain `ui.horizontal` centres its contents in that
/// leftover space and so puts each lane's icons on a different line. Pinning the
/// band to [`PRESET_ROW_HEIGHT`] makes every lane agree, and returns the band so
/// callers can anchor overlays to it. `min_width` keeps the cell's own column from
/// collapsing; the contents may still grow it.
pub(super) fn preset_row_band<R>(
    ui: &mut egui::Ui,
    min_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui, egui::Rect) -> R,
) -> egui::InnerResponse<R> {
    let cell = ui.available_rect_before_wrap();
    let band = egui::Rect::from_min_size(
        cell.min,
        egui::vec2(cell.width().max(min_width), PRESET_ROW_HEIGHT),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(band)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_min_size(egui::vec2(min_width, PRESET_ROW_HEIGHT));
            add_contents(ui, band)
        },
    )
}

pub(super) fn preset_action_id(preset_id: &str, action: &'static str) -> egui::Id {
    egui::Id::new("preset-overlay-action")
        .with(preset_id)
        .with(action)
}

/// Rightmost overlay slot inside a preset's content cell. `center_y` comes from
/// the row's own icon rather than the cell rect, whose height is the grid's
/// leftover space and therefore varies from column to column.
pub(super) fn trailing_overlay_slot(cell_right: f32, center_y: f32) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(cell_right - PRESET_OVERLAY_BUTTON / 2.0, center_y),
        egui::Vec2::splat(PRESET_OVERLAY_BUTTON),
    )
}

/// One chip spanning every trailing action, so the label cannot peek through the
/// gaps between them. Padded to the left only: the right edge stays on the cell
/// boundary that separates these actions from the star.
pub(super) fn overlay_bridge(rightmost: egui::Rect, count: usize) -> egui::Rect {
    let span = count.saturating_sub(1) as f32 * PRESET_OVERLAY_PITCH;
    egui::Rect::from_min_max(
        egui::pos2(rightmost.left() - span - 3.0, rightmost.top() - 1.0),
        egui::pos2(rightmost.right(), rightmost.bottom() + 1.0),
    )
}

/// Mask the label behind the trailing actions: the row's own background, then the
/// hotkey chip on top of it so tinted rows stay tinted edge to edge.
pub(super) fn paint_overlay_bridge(
    ui: &egui::Ui,
    bridge: egui::Rect,
    chip: Option<egui::Color32>,
    opacity: f32,
) {
    let painter = ui.painter();
    painter.rect_filled(bridge, 4.0, ui.visuals().panel_fill.gamma_multiply(opacity));
    if let Some(chip) = chip {
        painter.rect_filled(bridge, 4.0, chip.gamma_multiply(opacity));
    }
}

/// A row action that floats over the label's tail instead of claiming a column.
/// Unlike the drag handle it does not cross-fade with what it covers: it rides on
/// the opaque bridge painted underneath, at that bridge's shared `opacity`, so the
/// whole action group reveals as one.
pub(super) fn overlay_icon_button(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: Icon,
    id: egui::Id,
    opacity: f32,
) -> egui::Response {
    let response = ui.interact(rect, id, egui::Sense::click());
    if opacity <= f32::EPSILON {
        return response;
    }

    let painter = ui.painter();
    let color = if response.hovered() {
        painter.rect_filled(
            rect,
            4.0,
            ui.visuals().widgets.hovered.bg_fill.gamma_multiply(opacity),
        );
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    paint_icon(
        painter,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(crate::gui::icons::ICON_SM)),
        icon,
        color.gamma_multiply(opacity),
    );
    response
}

fn next_widget_proximity_opacity(ui: &egui::Ui, size: f32) -> f32 {
    let candidate_rect =
        egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(size, size));
    proximity_opacity(ui.input(|input| input.pointer.hover_pos()), candidate_rect)
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
    fn drag_morph_falls_off_radially_and_clears_before_the_next_row_but_one() {
        const ROW_PITCH: f32 = 26.0;
        let icon = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(18.0, 18.0));
        let morph = |x: f32, y: f32| {
            proximity_opacity_within(Some(egui::pos2(x, y)), icon, PRESET_DRAG_MORPH_RADIUS)
        };

        assert_eq!(
            proximity_opacity_within(None, icon, PRESET_DRAG_MORPH_RADIUS),
            0.0
        );
        assert_eq!(morph(icon.center().x, icon.center().y), 1.0);

        // Distance is measured from the rect in both axes at once: a diagonal
        // approach is farther than the same offset along one axis.
        let sideways = morph(icon.right() + 10.0, icon.center().y);
        let diagonal = morph(icon.right() + 10.0, icon.bottom() + 10.0);
        assert!(sideways > diagonal && diagonal > 0.0);

        // The reveal reaches well past its own row but still resolves to a clear
        // winner: the row under the pointer must lead its neighbour comfortably.
        let own = morph(icon.center().x, icon.center().y);
        let neighbour = morph(icon.center().x, icon.bottom() + ROW_PITCH);
        assert!(own - neighbour > 0.1, "neighbour: {neighbour}");
        assert!(morph(icon.center().x, icon.bottom() + 4.0 * ROW_PITCH) > 0.0);
        assert_eq!(
            morph(
                icon.center().x,
                icon.bottom() + PRESET_DRAG_MORPH_RADIUS + 1.0
            ),
            0.0
        );
    }

    #[test]
    fn overlay_actions_hug_the_cell_tail_and_clear_the_type_icon() {
        // The narrowest cell the layout can produce is the worst case for the
        // pair of overlay actions reaching back into the type icon.
        let cell = egui::Rect::from_min_size(
            egui::pos2(40.0, 200.0),
            egui::vec2(PRESET_CONTENT_MIN_WIDTH, PRESET_ROW_HEIGHT),
        );
        let row_center_y = cell.center().y;
        let delete = trailing_overlay_slot(cell.right(), row_center_y);
        let copy = delete.translate(egui::vec2(-PRESET_OVERLAY_PITCH, 0.0));

        assert_eq!(delete.right(), cell.right());
        assert_eq!(delete.center().y, row_center_y);
        assert_eq!(copy.center().y, row_center_y);
        let icon_slot_end = cell.left() + crate::gui::icons::ICON_LG + PRESET_CONTENT_GAP;
        assert!(
            copy.left() > icon_slot_end,
            "copy at {} overlaps the type icon ending at {icon_slot_end}",
            copy.left()
        );
    }

    #[test]
    fn overlay_actions_and_the_star_share_one_pitch_and_one_unbroken_backdrop() {
        let cell_right = 260.0;
        let row_center_y = 211.0;
        let delete = trailing_overlay_slot(cell_right, row_center_y);
        let copy = delete.translate(egui::vec2(-PRESET_OVERLAY_PITCH, 0.0));
        // The star is the first widget of the next grid column, so it starts one
        // column gap past the content cell.
        let star = egui::Rect::from_min_size(
            egui::pos2(cell_right + PRESET_GRID_COLUMN_GAP, row_center_y - 9.0),
            egui::Vec2::splat(crate::gui::icons::ICON_LG),
        );

        let pitches = [
            delete.center().x - copy.center().x,
            star.center().x - delete.center().x,
        ];
        assert_eq!(pitches[0], pitches[1], "uneven action pitch: {pitches:?}");
        assert_eq!(star.center().y, delete.center().y);

        // Nothing of the label may show between the two overlay actions.
        let bridge = overlay_bridge(delete, 2);
        assert!(bridge.contains_rect(copy) && bridge.contains_rect(delete));
        assert!(
            bridge.left() < copy.left(),
            "no lead-in padding for the mask"
        );
        assert_eq!(
            bridge.right(),
            delete.right(),
            "the mask must stop at the star's column"
        );
    }
}
