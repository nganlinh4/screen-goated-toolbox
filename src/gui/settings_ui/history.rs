use crate::config::Config;
use crate::config::types::{MAX_HISTORY_LIMIT, MIN_HISTORY_LIMIT};
use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use crate::history::{HistoryItem, HistoryManager};
use eframe::egui;

pub fn render_history_panel(
    ui: &mut egui::Ui,
    config: &mut Config,
    history_manager: &HistoryManager,
    search_query: &mut String,
    text: &LocaleText,
    content_bottom: f32,
) -> bool {
    let mut changed = false;

    let is_dark = ui.visuals().dark_mode;
    let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
    let card_bg = theme.card_bg();
    let card_stroke = theme.card_stroke();

    // The panel (a native side-panel) already bounds the width; don't force a
    // hardcoded max that exceeds it (that overran the panel and clipped the text).

    // === HEADER CARD ===
    ui.add_space(5.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            // Row 1: Title + Max items slider
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::History, Some(crate::gui::icons::ICON_SM));
                ui.label(
                    egui::RichText::new(text.workspace.history_title)
                        .strong()
                        .size(14.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let draft_id = ui.make_persistent_id("history_limit_draft");
                    let mut draft = ui.data(|data| {
                        data.get_temp::<HistoryLimitDraft>(draft_id)
                            .unwrap_or(HistoryLimitDraft {
                                value: config.max_history_items,
                                dragging: false,
                            })
                    });
                    if !draft.dragging {
                        draft.value = config.max_history_items;
                    }
                    let response = ui.add(egui::Slider::new(
                        &mut draft.value,
                        MIN_HISTORY_LIMIT..=MAX_HISTORY_LIMIT,
                    ));
                    let pointer_down = response.is_pointer_button_down_on();
                    let commit = draft.update(response.changed(), pointer_down);
                    ui.data_mut(|data| data.insert_temp(draft_id, draft));
                    if commit && config.max_history_items != draft.value {
                        config.max_history_items = draft.value;
                        history_manager.request_prune(config.max_history_items);
                        crate::overlay::generation_history::request_prune(config.max_history_items);
                        crate::overlay::input_history::request_prune(config.max_history_items);
                        changed = true;
                    }
                    ui.label(text.workspace.max_items_label);
                });
            });

            ui.add_space(6.0);

            // Computer Control conversation memory has its OWN retention cap so the
            // (numerous) media history items above can't push CC conversations out.
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::SmartToy, Some(crate::gui::icons::ICON_SM));
                ui.label(egui::RichText::new(text.workspace.cc_memory_max_label).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Slider::new(&mut config.cc_max_memory_items, 5..=100))
                        .changed()
                    {
                        changed = true;
                    }
                });
            });

            ui.add_space(8.0);

            // Row 2: Search + Actions
            ui.horizontal(|ui| {
                ui.scope(|ui| {
                    if !is_dark {
                        let visuals = ui.visuals_mut();
                        visuals.extreme_bg_color = egui::Color32::from_gray(242);
                        visuals.widgets.inactive.bg_stroke =
                            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(220));
                        visuals.widgets.hovered.bg_stroke =
                            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200));
                        visuals.widgets.active.bg_stroke =
                            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(180));
                    }
                    ui.add(
                        egui::TextEdit::singleline(search_query)
                            .hint_text(text.preset_basics.search_placeholder)
                            .desired_width(220.0),
                    );
                });

                if !search_query.is_empty()
                    && icon_button(ui, Icon::Close)
                        .on_hover_text(text.overlay.history_clear_search_tooltip)
                        .clicked()
                {
                    *search_query = "".to_string();
                }

                if icon_button(ui, Icon::Folder)
                    .on_hover_text(text.overlay.history_open_media_folder_tooltip)
                    .clicked()
                {
                    let config_dir = crate::paths::app_runtime_config_dir().join("history_media");
                    let _ = std::fs::create_dir_all(&config_dir);
                    let _ = open::that(config_dir);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Clear All button — destructive action via the shared Material
                    // filled-button helper (keeps hover/press state layers).
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.workspace.clear_all_history_btn,
                        theme.danger_fill(),
                        theme.on_accent(),
                        8,
                    )
                    .clicked()
                    {
                        history_manager.clear_all();
                        crate::overlay::input_history::clear_all();
                        crate::overlay::computer_control::clear_retained_history();
                    }
                });
            });
        });

    ui.add_space(8.0);

    if history_manager.persistence_failed() {
        let message = if history_manager.load_failed() {
            text.workspace.history_load_failed
        } else {
            text.workspace.history_write_failed
        };
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
    if history_manager.load_failed() {
        return changed;
    }
    let content_key = history_manager.content_key();
    let filtered = FILTERED.with_borrow_mut(|cache| {
        cache.get(content_key, search_query, || history_manager.snapshot())
    });

    if filtered.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(text.workspace.history_empty);
        });
    } else {
        // History items in scroll area — fill the remaining height down to the
        // footer. Measure from the panel's true bottom (`content_bottom`); the
        // column's `available_height()` reports a capped value here.
        let list_h = (content_bottom - ui.cursor().top() - 16.0).max(300.0);
        egui::Frame::new().show(ui, |ui| {
            ui.set_height(list_h);

            egui::ScrollArea::vertical().show(ui, |ui| {
                super::history_list::render_history_rows(
                    ui,
                    &filtered,
                    content_key,
                    history_manager,
                    text,
                    &super::history_list::ListStyle {
                        is_dark,
                        card_stroke,
                    },
                );
            });
        });
    }

    changed
}

thread_local! {
    static FILTERED: std::cell::RefCell<FilteredHistory> =
        std::cell::RefCell::new(FilteredHistory::default());
}

/// The search result for one (history content, query) pair. Copying and
/// lowercasing every item is done once per change, not once per frame.
#[derive(Default)]
struct FilteredHistory {
    key: Option<(u64, String)>,
    items: std::rc::Rc<Vec<HistoryItem>>,
}

impl FilteredHistory {
    fn get(
        &mut self,
        content_key: u64,
        query: &str,
        snapshot: impl FnOnce() -> Vec<HistoryItem>,
    ) -> std::rc::Rc<Vec<HistoryItem>> {
        let current = self
            .key
            .as_ref()
            .is_some_and(|(key, cached)| *key == content_key && cached == query);
        if !current {
            let lowercase = query.to_lowercase();
            let items = snapshot()
                .into_iter()
                .filter(|item| item.matches_query(&lowercase))
                .collect();
            self.items = std::rc::Rc::new(items);
            self.key = Some((content_key, query.to_owned()));
        }
        self.items.clone()
    }
}

#[derive(Clone, Copy)]
struct HistoryLimitDraft {
    value: usize,
    dragging: bool,
}

impl HistoryLimitDraft {
    fn update(&mut self, changed: bool, pointer_down: bool) -> bool {
        let released = self.dragging && !pointer_down;
        self.dragging = pointer_down;
        released || (changed && !pointer_down)
    }
}

#[cfg(test)]
mod tests {
    use super::{FilteredHistory, HistoryLimitDraft, MAX_HISTORY_LIMIT, MIN_HISTORY_LIMIT};
    use crate::history::{HistoryItem, HistoryType};

    fn item(id: i64, text: &str) -> HistoryItem {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "timestamp": "2026-01-01 00:00",
            "item_type": "Text",
            "text": text,
            "media_path": "",
        }))
        .unwrap()
    }

    #[test]
    fn history_search_is_recomputed_only_when_content_or_query_changes() {
        let mut cache = FilteredHistory::default();
        let mut snapshots = 0;
        let mut take = |items: Vec<HistoryItem>| {
            snapshots += 1;
            items
        };
        let all = vec![item(1, "Alpha"), item(2, "beta")];
        assert_eq!(cache.get(7, "ALP", || take(all.clone())).len(), 1);
        assert_eq!(cache.get(7, "ALP", || take(all.clone())).len(), 1);
        assert_eq!(cache.get(7, "", || take(all.clone())).len(), 2);
        assert_eq!(
            cache.get(8, "", || take(vec![item(3, "gamma")]))[0].text,
            "gamma"
        );
        assert_eq!(snapshots, 3);
        assert_eq!(cache.items[0].item_type, HistoryType::Text);
    }

    #[test]
    fn history_limit_drag_commits_only_after_release() {
        let mut draft = HistoryLimitDraft {
            value: MAX_HISTORY_LIMIT,
            dragging: false,
        };
        draft.value = MIN_HISTORY_LIMIT;
        assert!(!draft.update(true, true));
        draft.value = 150;
        assert!(!draft.update(true, true));
        assert!(draft.update(false, false));
        assert!(!draft.update(false, false));
        draft.value = 151;
        assert!(draft.update(true, false));
    }
}
