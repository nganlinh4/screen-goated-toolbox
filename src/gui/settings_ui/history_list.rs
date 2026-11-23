//! The History list body: bounded previews and offscreen-row skipping.
//!
//! Long entries show a preview of at most `PREVIEW_LINES` lines and
//! `PREVIEW_CHARS` characters until expanded. A row that has been laid out
//! once remembers its height; while it is offscreen only that much space is
//! reserved, so a scroll frame lays out just the rows it can show.

use std::collections::{HashMap, HashSet};

use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use crate::history::{HistoryItem, HistoryManager, HistoryType};
use eframe::egui;

const PREVIEW_LINES: usize = 4;
const PREVIEW_CHARS: usize = 320;
const ROW_GAP: f32 = 4.0;

/// The start of `text` to show collapsed, or `None` when it fits whole.
fn preview(text: &str) -> Option<&str> {
    let mut lines = 1;
    for (chars, (index, ch)) in text.char_indices().enumerate() {
        if chars == PREVIEW_CHARS {
            return Some(text[..index].trim_end());
        }
        if ch == '\n' {
            if lines == PREVIEW_LINES {
                return Some(text[..index].trim_end());
            }
            lines += 1;
        }
    }
    None
}

/// Per-session list state: which rows are expanded, and the laid-out height
/// of each row at the current width.
#[derive(Default)]
struct ListState {
    width: f32,
    content_key: u64,
    heights: HashMap<(i64, bool), f32>,
    expanded: HashSet<i64>,
}

impl ListState {
    /// Forgets heights that no longer hold: the width or the items changed.
    fn begin_frame(&mut self, width: f32, content_key: u64) {
        if (self.width - width).abs() > 0.5 || self.content_key != content_key {
            self.heights.clear();
            self.width = width;
            self.content_key = content_key;
        }
    }

    /// The space to reserve for a row instead of laying it out, when its
    /// height is known and that space is not visible.
    fn skip(&self, id: i64, top: f32, visible: egui::Rangef) -> Option<f32> {
        let height = *self.heights.get(&(id, self.expanded.contains(&id)))?;
        let bottom = top + height;
        (bottom < visible.min || top > visible.max).then_some(height)
    }

    fn record(&mut self, id: i64, height: f32) {
        let expanded = self.expanded.contains(&id);
        self.heights.insert((id, expanded), height);
    }

    fn toggle(&mut self, id: i64) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }
}

thread_local! {
    static LIST: std::cell::RefCell<ListState> = std::cell::RefCell::new(ListState::default());
}

pub(super) struct ListStyle {
    pub is_dark: bool,
    pub card_stroke: egui::Stroke,
}

/// Renders the rows inside the History scroll area.
pub(super) fn render_history_rows(
    ui: &mut egui::Ui,
    items: &[HistoryItem],
    content_key: u64,
    history_manager: &HistoryManager,
    text: &LocaleText,
    style: &ListStyle,
) {
    let visible = ui.clip_rect().y_range();
    let width = ui.available_width();
    let mut id_to_delete = None;
    LIST.with_borrow_mut(|list| {
        list.begin_frame(width, content_key);
        for item in items {
            let top = ui.cursor().top();
            if let Some(height) = list.skip(item.id, top, visible) {
                ui.add_space(height + ui.spacing().item_spacing.y + ROW_GAP);
                continue;
            }
            let expanded = list.expanded.contains(&item.id);
            let row = render_row(ui, item, expanded, history_manager, text, style);
            if row.delete {
                id_to_delete = Some(item.id);
            }
            if row.toggle {
                list.toggle(item.id);
                ui.ctx().request_repaint();
            } else {
                list.record(item.id, row.height);
            }
            ui.add_space(ROW_GAP);
        }
    });
    if let Some(id) = id_to_delete {
        history_manager.delete(id);
    }
}

struct RowOutcome {
    height: f32,
    delete: bool,
    toggle: bool,
}

fn row_fill(item_type: &HistoryType, is_dark: bool) -> egui::Color32 {
    let (dark, light) = match item_type {
        // Subtle blue tint for images
        HistoryType::Image => ((30, 38, 52), (240, 245, 255)),
        // Subtle green tint for text
        HistoryType::Text => ((30, 42, 38), (240, 252, 245)),
        // Subtle orange/amber tint for audio
        HistoryType::Audio => ((42, 36, 30), (255, 250, 240)),
    };
    let ((r, g, b), alpha) = if is_dark { (dark, 235) } else { (light, 255) };
    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

fn render_row(
    ui: &mut egui::Ui,
    item: &HistoryItem,
    expanded: bool,
    history_manager: &HistoryManager,
    text: &LocaleText,
    style: &ListStyle,
) -> RowOutcome {
    let mut outcome = RowOutcome {
        height: 0.0,
        delete: false,
        toggle: false,
    };
    let response = egui::Frame::new()
        .fill(row_fill(&item.item_type, style.is_dark))
        .stroke(style.card_stroke)
        .inner_margin(8.0)
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon = match item.item_type {
                    HistoryType::Image => Icon::Image,
                    HistoryType::Audio => Icon::Microphone,
                    HistoryType::Text => Icon::Text,
                };
                draw_icon_static(ui, icon, Some(crate::gui::icons::ICON_SM));
                ui.label(egui::RichText::new(&item.timestamp).size(10.0).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    outcome.delete = render_row_actions(ui, item, history_manager, text);
                });
            });

            let preview = preview(&item.text);
            let shown = match preview {
                Some(preview) if !expanded => format!("{preview}…"),
                _ => item.text.clone(),
            };
            ui.label(egui::RichText::new(shown).size(13.0));
            if preview.is_some() {
                let label = if expanded {
                    text.workspace.history_show_less
                } else {
                    text.workspace.history_show_more
                };
                outcome.toggle = ui.link(label).clicked();
            }
        })
        .response;
    outcome.height = response.rect.height();
    outcome
}

/// Delete, copy and open-media buttons; returns whether delete was clicked.
fn render_row_actions(
    ui: &mut egui::Ui,
    item: &HistoryItem,
    history_manager: &HistoryManager,
    text: &LocaleText,
) -> bool {
    let delete = icon_button(ui, Icon::DeleteLarge)
        .on_hover_text(text.overlay.history_delete_tooltip)
        .clicked();

    if icon_button(ui, Icon::Copy)
        .on_hover_text(text.overlay.history_copy_text_tooltip)
        .clicked()
    {
        crate::gui::utils::copy_to_clipboard_text(&item.text);
    }

    if !item.media_path.is_empty() {
        let btn_text = match item.item_type {
            HistoryType::Image => text.workspace.view_image_btn,
            HistoryType::Audio => text.workspace.listen_audio_btn,
            HistoryType::Text => text.workspace.view_text_btn,
        };
        if ui.button(btn_text).clicked() {
            let config_dir = crate::paths::app_runtime_config_dir().join("history_media");
            if let Some(path) = history_manager.media_path_for(&config_dir, item) {
                let _ = open::that(path);
            }
        }
    }
    delete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_entries_show_whole_and_long_ones_are_bounded() {
        assert_eq!(preview("one line"), None);
        assert_eq!(preview("a\nb\nc\nd"), None);
        assert_eq!(preview("a\nb\nc\nd\ne"), Some("a\nb\nc\nd"));
        let long = "word ".repeat(200);
        let cut = preview(&long).unwrap();
        assert!(cut.chars().count() <= PREVIEW_CHARS);
        // Multi-byte text is cut on a character boundary.
        let korean = "가".repeat(PREVIEW_CHARS + 5);
        assert_eq!(preview(&korean).unwrap().chars().count(), PREVIEW_CHARS);
        assert_eq!(preview(&"가".repeat(PREVIEW_CHARS)), None);
    }

    #[test]
    fn only_measured_offscreen_rows_are_skipped() {
        let mut list = ListState::default();
        list.begin_frame(400.0, 1);
        let visible = egui::Rangef::new(1000.0, 1600.0);
        // Never laid out: must be rendered once to learn its height.
        assert_eq!(list.skip(7, 0.0, visible), None);
        list.record(7, 120.0);
        assert_eq!(list.skip(7, 0.0, visible), Some(120.0));
        assert_eq!(list.skip(7, 2000.0, visible), Some(120.0));
        // Overlapping the viewport: rendered.
        assert_eq!(list.skip(7, 900.0, visible), None);
        assert_eq!(list.skip(7, 1500.0, visible), None);

        // Expanding changes the height, so the row is measured again.
        list.toggle(7);
        assert_eq!(list.skip(7, 0.0, visible), None);
        list.record(7, 480.0);
        assert_eq!(list.skip(7, 0.0, visible), Some(480.0));
        list.toggle(7);
        assert_eq!(list.skip(7, 0.0, visible), Some(120.0));

        // A new width or new history content invalidates measured heights.
        list.begin_frame(300.0, 1);
        assert_eq!(list.skip(7, 0.0, visible), None);
        list.record(7, 150.0);
        list.begin_frame(300.0, 2);
        assert_eq!(list.skip(7, 0.0, visible), None);
    }
}
