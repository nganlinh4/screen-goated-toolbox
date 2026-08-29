use crate::gui::icons::{self, Icon};
use crate::gui::theme::blend;
use eframe::egui::{self, Color32, CornerRadius};
use std::sync::Arc;

const LABEL_FONT_SIZE: f32 = 11.0;
const LABEL_LINE_HEIGHT: f32 = 12.0;
const ICON_GAP: f32 = 3.0;
pub(super) const HORIZONTAL_PADDING: f32 = 5.0;
const BALANCE_SEARCH_PASSES: usize = 12;
const CORNER_RADIUS: u8 = 9;

/// A dense footer launcher with a larger glyph and a balanced two-line label.
///
/// The wrap width is solved from the rendered glyphs rather than from words or
/// byte counts. This lets egui choose natural boundaries for spaced scripts,
/// CJK text, punctuation, and future translations through one path.
pub(super) fn footer_launcher_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let label_galley = balanced_two_line_label(ui, label, text);
    let icon_size = icons::ICON_XL;
    let h_pad = ui.spacing().button_padding.x.max(HORIZONTAL_PADDING);
    let button_size = egui::vec2(
        h_pad + icon_size + ICON_GAP + label_galley.rect.width() + h_pad,
        ui.spacing()
            .interact_size
            .y
            .max(label_galley.rect.height() + ui.spacing().button_padding.y * 2.0),
    );
    let (button_rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

    let surface = if response.is_pointer_button_down_on() {
        blend(fill, text, 0.14)
    } else if response.hovered() {
        blend(fill, text, 0.08)
    } else {
        fill
    };

    let painter = ui.painter();
    painter.rect_filled(button_rect, CornerRadius::same(CORNER_RADIUS), surface);

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            button_rect.left() + h_pad,
            button_rect.center().y - icon_size / 2.0,
        ),
        egui::Vec2::splat(icon_size),
    );
    icons::paint_icon(painter, icon_rect, icon, text);

    // Center-aligned galleys use x=0 as their horizontal anchor.
    painter.galley(
        egui::pos2(
            icon_rect.right() + ICON_GAP + label_galley.rect.width() / 2.0,
            button_rect.center().y - label_galley.rect.height() / 2.0,
        ),
        label_galley,
        text,
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
}

fn balanced_two_line_label(ui: &egui::Ui, label: &str, text: Color32) -> Arc<egui::Galley> {
    let font = egui::FontId::proportional(LABEL_FONT_SIZE);
    let unwrapped = label_galley(ui, label.to_owned(), text, font.clone(), f32::INFINITY);

    if unwrapped.rows.len() > 1 || label.chars().count() < 2 {
        return unwrapped;
    }

    if let Some(galley) = balanced_word_boundary_label(ui, label, text, font.clone()) {
        return galley;
    }

    // Labels without whitespace (for example CJK or a single long word) still
    // use egui's script-aware break selection. Find the narrowest text column
    // that fits the complete label in two rows.
    let mut too_narrow = 0.0;
    let mut fits_two_rows = unwrapped.rect.width().max(1.0);
    for _ in 0..BALANCE_SEARCH_PASSES {
        let candidate = (too_narrow + fits_two_rows) / 2.0;
        let galley = wrapped_label(ui, label, text, font.clone(), candidate);
        if galley.rows.len() > 2 {
            too_narrow = candidate;
        } else {
            fits_two_rows = candidate;
        }
    }

    wrapped_label(ui, label, text, font, fits_two_rows.ceil())
}

fn balanced_word_boundary_label(
    ui: &egui::Ui,
    label: &str,
    text: Color32,
    font: egui::FontId,
) -> Option<Arc<egui::Galley>> {
    let words: Vec<&str> = label.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }

    let mut best: Option<(f32, f32, String)> = None;
    for split in 1..words.len() {
        let first = words[..split].join(" ");
        let second = words[split..].join(" ");
        let first_width = ui
            .painter()
            .layout_no_wrap(first.clone(), font.clone(), text)
            .rect
            .width();
        let second_width = ui
            .painter()
            .layout_no_wrap(second.clone(), font.clone(), text)
            .rect
            .width();
        let widest = first_width.max(second_width);
        let imbalance = (first_width - second_width).abs();
        let candidate = format!("{first}\n{second}");

        if best
            .as_ref()
            .is_none_or(|(best_widest, best_imbalance, _)| {
                widest < *best_widest || (widest == *best_widest && imbalance < *best_imbalance)
            })
        {
            best = Some((widest, imbalance, candidate));
        }
    }

    best.map(|(_, _, balanced)| centered_label(ui, balanced, text, font))
}

fn centered_label(
    ui: &egui::Ui,
    label: String,
    text: Color32,
    font: egui::FontId,
) -> Arc<egui::Galley> {
    label_galley(ui, label, text, font, f32::INFINITY)
}

fn wrapped_label(
    ui: &egui::Ui,
    label: &str,
    text: Color32,
    font: egui::FontId,
    width: f32,
) -> Arc<egui::Galley> {
    label_galley(ui, label.to_owned(), text, font, width)
}

fn label_galley(
    ui: &egui::Ui,
    label: String,
    text: Color32,
    font: egui::FontId,
    width: f32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(label, font, text, width);
    job.halign = egui::Align::Center;
    for section in &mut job.sections {
        section.format.line_height = Some(LABEL_LINE_HEIGHT);
    }
    ui.painter().layout_job(job)
}

#[cfg(test)]
pub(super) fn footer_label_row_count(ui: &egui::Ui, label: &str) -> usize {
    balanced_two_line_label(ui, label, Color32::WHITE)
        .rows
        .len()
}

#[cfg(test)]
pub(super) fn footer_label_rows(ui: &egui::Ui, label: &str) -> Vec<String> {
    balanced_two_line_label(ui, label, Color32::WHITE)
        .rows
        .iter()
        .map(|row| row.glyphs.iter().map(|glyph| glyph.chr).collect())
        .collect()
}

#[cfg(test)]
pub(super) fn footer_label_height(ui: &egui::Ui, label: &str) -> f32 {
    balanced_two_line_label(ui, label, Color32::WHITE)
        .rect
        .height()
}
