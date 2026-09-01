use eframe::egui;

use crate::gui::theme::AppTheme;

use super::{Action, activated, paint_card, paint_elided_text};
use crate::overlay::tray_popup::data::PopupSnapshot;
use crate::overlay::tray_popup::layout::{FLYOUT_WIDTH, OPTION_HEIGHT, PopupPlacement};

pub(super) struct PaintResult {
    pub action: Option<Action>,
    pub hovered: bool,
}

pub(super) fn paint(
    ui: &mut egui::Ui,
    placement: PopupPlacement,
    snapshot: &PopupSnapshot,
    theme: AppTheme,
) -> PaintResult {
    let size = egui::vec2(FLYOUT_WIDTH, placement.flyout_height);
    ui.set_min_size(size);
    let rect = egui::Rect::from_min_size(ui.max_rect().min, size);
    paint_card(ui.painter(), rect, theme, ui.visuals().dark_mode);

    let mut action = None;
    let mut hovered = rect.contains(
        ui.input(|input| input.pointer.hover_pos())
            .unwrap_or_default(),
    );
    for (index, option) in snapshot.restore_options.iter().enumerate() {
        let option_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(4.0, 4.0 + index as f32 * OPTION_HEIGHT),
            egui::vec2(FLYOUT_WIDTH - 8.0, OPTION_HEIGHT),
        );
        let response = ui.interact(
            option_rect,
            ui.id().with(("restore-option", option.batch_count)),
            egui::Sense::click(),
        );
        hovered |= response.hovered();
        if response.hovered() || response.has_focus() {
            ui.painter().rect_filled(
                option_rect,
                egui::CornerRadius::same(4),
                theme.neutral_fill(),
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                option_rect.shrink(1.0),
                egui::CornerRadius::same(4),
                egui::Stroke::new(1.0, theme.accent_fill()),
                egui::StrokeKind::Inside,
            );
        }
        paint_elided_text(
            ui.painter(),
            &option.label,
            egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
            option_rect.width() - 20.0,
            12.0,
            theme.on_surface(),
        );
        if activated(ui, &response) {
            action = Some(Action::Restore(option.batch_count));
        }
    }
    PaintResult { action, hovered }
}
