//! The spacing scale and the standard control height.
//!
//! Kept apart from the palette: these decide *where* things sit, and every
//! surface in the app reads them.

use eframe::egui::Color32;

/// The app's spacing scale.
///
/// Every frame margin and edge inset comes from here. Before this existed the
/// codebase held 13 distinct margin literals across 30 sites — `(10, 8)`,
/// `(10, 9)`, `(10, 6)` and `(10, 5)` were four different answers to the same
/// question — so nominally identical surfaces sat at visibly different insets
/// and a window corner showed two different "distances to the edge" at once.
///
/// Pick by role, not by number, and prefer a symmetric [`Margin::same`] for
/// anything that meets a window or panel edge: an inset that differs between
/// the side and the bottom is exactly what the eye catches at a corner.
pub mod space {
    /// Hairline padding, for table rows whose height is set explicitly.
    pub const MICRO: i8 = 1;
    /// Inside a chip, badge, or table cell.
    pub const HAIR: i8 = 2;
    /// Inside a dense row or compact pill.
    pub const TIGHT: i8 = 4;
    /// Inside a bar, or a small card holding one row.
    pub const SNUG: i8 = 6;
    /// Between grouped content and the container around it.
    pub const GAP: i8 = 8;
    /// Content inset from a panel edge.
    pub const EDGE: i8 = 10;
    /// Inside a card that holds several rows.
    pub const CARD: i8 = 12;
    /// Inside a modal dialog surface.
    pub const DIALOG: i8 = 20;
}

/// Distance between the window frame and anything drawn against it.
///
/// The title bar, the launch bar, the preset column, and the detail columns
/// all sit this far in. They used to disagree — 8px on the body's left, 18px on
/// its right, 6px at the bars — which reads as the whole window being off
/// centre. One number is the only way that stays true as surfaces are added.
pub const WINDOW_EDGE_INSET: i8 = space::SNUG;

/// Height of a standard control — pill, chip, button, combo, single-line edit.
///
/// Also the minimum height of any row, so short content centres against the
/// same box a control would occupy. See `apply_global_style`.
pub const CONTROL_HEIGHT: f32 = 22.0;

/// Linear blend from `a` toward `b` by `t` (0.0 = `a`, 1.0 = `b`).
///
/// Used to build Material state layers — overlay the on-color over a fill to get
/// hover (≈8%) and pressed (≈14%) variants that read correctly in both themes.
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

#[cfg(test)]
mod row_alignment_tests {
    use super::*;
    use crate::gui::theme::AppTheme;
    use eframe::egui::{Color32, Context, Ui};

    /// A label written before a taller control must share its centre.
    ///
    /// This is the invariant `spacing.interact_size.y` buys us: egui centres a
    /// widget against the row height at the moment it is added, so without it
    /// the label is centred in a shorter row than the one it ends up in. The
    /// regression is invisible in code review and shows up as every label in
    /// the app sitting ~2px high, so it is pinned here.
    #[test]
    fn a_label_shares_the_row_centre_with_a_control_added_after_it() {
        let ctx = Context::default();
        AppTheme::apply_global_style(&ctx, true);
        // First pass loads fonts; measure on the second.
        let _ = crate::gui::test_support::run_ui(&ctx, Default::default(), |_| {});

        let mut label_centre = 0.0;
        let mut control_centre = 0.0;
        let _ = crate::gui::test_support::run_ui(&ctx, Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    label_centre = ui
                        .label(eframe::egui::RichText::new("Label").size(12.5))
                        .rect
                        .center()
                        .y;
                    control_centre = ui
                        .add(
                            eframe::egui::Button::new(
                                eframe::egui::RichText::new("Control").size(12.5),
                            )
                            .min_size(eframe::egui::vec2(0.0, CONTROL_HEIGHT)),
                        )
                        .rect
                        .center()
                        .y;
                });
            });
        });

        assert!(
            (label_centre - control_centre).abs() < 0.5,
            "label centre {label_centre} vs control centre {control_centre}"
        );
    }

    /// Small copy beside a dialog title must share the title's baseline.
    ///
    /// Centre-aligning boxes of different text sizes puts the smaller one's
    /// baseline ~2px high, so `widgets::baseline_aligned` nudges it back down.
    #[test]
    fn dialog_description_sits_on_the_title_baseline() {
        use crate::gui::widgets::{DIALOG_DESCRIPTION_SIZE, DIALOG_TITLE_SIZE, baseline_aligned};

        let ctx = Context::default();
        AppTheme::apply_global_style(&ctx, true);
        crate::gui::utils::configure_fonts(&ctx);
        let _ = crate::gui::test_support::run_ui(&ctx, Default::default(), |_| {});

        let mut title_baseline = 0.0;
        let mut description_baseline = 0.0;
        let _ = crate::gui::test_support::run_ui(&ctx, Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let title =
                        ui.label(eframe::egui::RichText::new("Title").size(DIALOG_TITLE_SIZE));
                    title_baseline = title.rect.top() + baseline_within(ui, DIALOG_TITLE_SIZE);
                    baseline_aligned(ui, DIALOG_TITLE_SIZE, DIALOG_DESCRIPTION_SIZE, |ui| {
                        let desc = ui.label(
                            eframe::egui::RichText::new("Supporting copy")
                                .size(DIALOG_DESCRIPTION_SIZE),
                        );
                        description_baseline =
                            desc.rect.top() + baseline_within(ui, DIALOG_DESCRIPTION_SIZE);
                    });
                });
            });
        });

        assert!(
            (title_baseline - description_baseline).abs() <= 1.0,
            "title baseline {title_baseline} vs description baseline {description_baseline}"
        );
    }

    /// Distance from a galley's top to its baseline, at `size` points.
    fn baseline_within(ui: &Ui, size: f32) -> f32 {
        let font = eframe::egui::FontId::new(size, eframe::egui::FontFamily::Proportional);
        ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap("H".to_owned(), font, Color32::WHITE)
                .rows[0]
                .glyphs[0]
                .pos
                .y
        })
    }
}
