use crate::config::{Config, ModelPriorityChains};
use crate::gui::icons::Icon;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::list_reorder::{ListReorder, Slot};
use crate::gui::settings_ui::model_selector;
use crate::gui::theme::{AppTheme, blend};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui::{self, Color32, CornerRadius, Margin};

const STEP_ROW_HEIGHT: f32 = 24.0;
const STEP_NUMBER_WIDTH: f32 = 16.0;
const STEP_SETTLE_SECONDS: f32 = 0.14;

/// Renders the Model Priority tab body (chain columns only).
///
/// Modal chrome, title, and description are owned by the shared models hub.
pub fn render_model_priority_body(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.columns(2, |columns| {
        if render_chain_section(
            &mut columns[0],
            &mut config.model_priority_chains.image_to_text,
            RetryChainKind::ImageToText,
            &config.ui_language,
            text,
        ) {
            changed = true;
        }

        if render_chain_section(
            &mut columns[1],
            &mut config.model_priority_chains.text_to_text,
            RetryChainKind::TextToText,
            &config.ui_language,
            text,
        ) {
            changed = true;
        }
    });
    changed
}

/// One retry chain rendered as a numbered ladder.
///
/// Every step — the pinned first pick, each editable fallback, and the trailing
/// auto step — uses the same pill geometry, so the chain reads top-to-bottom as
/// one ordered sequence instead of three differently shaped blocks. Rows
/// alternate a faint tint for scanability, and the reorder cluster occupies a
/// fixed-width slot at the right edge so both chains line up column to column.
fn render_chain_section(
    ui: &mut egui::Ui,
    chain: &mut Vec<String>,
    chain_kind: RetryChainKind,
    ui_language: &str,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let section_title = match chain_kind {
        RetryChainKind::ImageToText => text.model_catalog.model_priority_image_chain_title,
        RetryChainKind::TextToText => text.model_catalog.model_priority_text_chain_title,
    };
    let section_id = match chain_kind {
        RetryChainKind::ImageToText => "model_priority_image_chain",
        RetryChainKind::TextToText => "model_priority_text_chain",
    };
    let theme = AppTheme::from_ui(ui);
    let section_title_color = match chain_kind {
        RetryChainKind::ImageToText => theme.node_special_title(),
        RetryChainKind::TextToText => theme.on_surface(),
    };

    egui::Frame::new()
        .fill(theme.card_bg())
        .stroke(theme.card_stroke())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(crate::gui::theme::space::EDGE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;

            ui.horizontal(|ui| {
                crate::gui::icons::arrow_label(
                    ui,
                    section_title,
                    Some(section_title_color),
                    |rt| rt.strong().size(13.0).color(section_title_color),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(text.preset_basics.reset_defaults_btn)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        let defaults = ModelPriorityChains::default();
                        *chain = match chain_kind {
                            RetryChainKind::ImageToText => defaults.image_to_text,
                            RetryChainKind::TextToText => defaults.text_to_text,
                        };
                        changed = true;
                    }
                });
            });
            ui.add_space(6.0);

            // Step 1: the user's explicit pick, locked to the head of the chain.
            step_row(ui, &theme, 1, false, |ui| {
                fixed_step_label(
                    ui,
                    &theme,
                    text.model_catalog.model_priority_chosen_model,
                    text.model_catalog.model_priority_fixed_hint,
                );
            });

            let keys_id = egui::Id::new((section_id, "step-keys"));
            let mut keys: StepKeys = ui.data(|data| data.get_temp(keys_id)).unwrap_or_default();
            keys.sync(chain.len());
            let mut reorder = ListReorder::load(ui, section_id);
            let lifting = reorder.is_lifting();

            let mut removal: Option<usize> = None;
            let mut lift_request: Option<(usize, egui::Rect)> = None;
            for (position, slot) in reorder.plan(chain.len()).into_iter().enumerate() {
                let step_number = position + 2;
                let tinted = position.is_multiple_of(2);
                let chain_idx = match slot {
                    Slot::Gap => {
                        reorder.note_slot(landing_gap(ui, &theme, &reorder, tinted));
                        continue;
                    }
                    Slot::Step(idx) => idx,
                };

                let key = keys.at(chain_idx);
                // Steps glide to the slot the gap pushed them into, so the list
                // parts for the carried step instead of jumping around it.
                let transform = egui::emath::TSTransform::from_translation(animated_step_offset(
                    ui, section_id, key, lifting,
                ));
                let mut remove_clicked = false;
                let mut grip = None;
                let row_rect = ui
                    .with_visual_transform(transform, |ui| {
                        step_row(ui, &theme, step_number, tinted, |ui| {
                            changed |= model_selector::render_model_combo(
                                ui,
                                (section_id, "combo", key),
                                &mut chain[chain_idx],
                                chain_kind,
                                ui_language,
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if reorder_clicked(ui, Icon::Close, true) {
                                        remove_clicked = true;
                                    }
                                    grip = Some(drag_grip(
                                        ui,
                                        egui::Id::new((section_id, "chain-grip", key)),
                                        lifting,
                                    ));
                                },
                            );
                        })
                    })
                    .inner;

                reorder.note_slot(row_rect);
                if remove_clicked {
                    removal = Some(chain_idx);
                }
                if grip.is_some_and(|grip| grip.drag_started()) {
                    lift_request = Some((chain_idx, row_rect));
                }
            }

            if let Some((from, row_rect)) = lift_request {
                reorder.lift(ui, from, row_rect);
            }
            draw_lifted_step(ui, &theme, &reorder, section_id, chain, ui_language);

            // Both mutations land after the pass: rewriting a chain the loop is
            // still walking would skip or repeat a step.
            if let Some(idx) = removal {
                chain.remove(idx);
                keys.forget(idx);
                changed = true;
            } else if let Some((from, to)) = reorder.settle(ui, chain.len()) {
                let step = chain.remove(from);
                chain.insert(to, step);
                keys.move_step(from, to);
                changed = true;
            }
            ui.data_mut(|data| data.insert_temp(keys_id, keys));
            reorder.store(ui);

            // Tail step: everything else, in smart fallback order.
            step_row(
                ui,
                &theme,
                chain.len() + 2,
                chain.len().is_multiple_of(2),
                |ui| {
                    fixed_step_label(
                        ui,
                        &theme,
                        text.model_catalog.model_priority_auto,
                        text.model_catalog.model_priority_auto_hint,
                    );
                },
            );

            ui.add_space(6.0);
            if ui
                .add_sized(
                    egui::vec2(ui.available_width(), 26.0),
                    egui::Button::new(
                        egui::RichText::new(text.model_catalog.model_priority_add_model)
                            .size(12.5)
                            .color(theme.on_surface_variant()),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(theme.card_stroke())
                    .corner_radius(CornerRadius::same(8)),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                chain.push(model_selector::default_model_id(chain_kind));
                changed = true;
            }
        });

    changed
}

/// Shared geometry for every chain step: a leading step number, then the
/// caller's content, inside an optionally tinted pill.
fn step_row(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    step: usize,
    tinted: bool,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let fill = if tinted {
        blend(theme.card_bg(), theme.on_surface(), 0.04)
    } else {
        Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(
            crate::gui::theme::space::SNUG,
            crate::gui::theme::space::TIGHT,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(STEP_ROW_HEIGHT);
                ui.add_sized(
                    egui::vec2(STEP_NUMBER_WIDTH, STEP_ROW_HEIGHT),
                    egui::Label::new(
                        egui::RichText::new(format!("{step}"))
                            .size(11.5)
                            .color(theme.on_surface_variant()),
                    ),
                );
                content(ui);
            });
        })
        .response
        .rect
}

/// The two non-editable steps (pinned pick, auto tail) share this treatment: a
/// strong name and muted supporting copy, deliberately icon-free so the ladder
/// has one visual anchor per row instead of two.
fn fixed_step_label(ui: &mut egui::Ui, theme: &AppTheme, label: &str, hint: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(12.5)
            .strong()
            .color(theme.on_surface()),
    );
    ui.label(
        egui::RichText::new(hint)
            .size(10.5)
            .color(theme.on_surface_variant()),
    );
}

/// Position-independent identity for each step of a chain.
///
/// A chain is a plain `Vec<String>` whose entries may repeat, so a step's egui id
/// can come from neither its value nor its index: keying by index means the id
/// changes the instant a row moves, which drops the in-flight drag and makes the
/// ladder flicker. Each step instead gets a serial number that travels with it
/// through reorders, exactly as a preset row is identified by its preset id.
#[derive(Clone, Default)]
struct StepKeys {
    next: u64,
    keys: Vec<u64>,
}

impl StepKeys {
    /// Hand out keys for steps added since the last frame and drop any trailing
    /// keys whose steps disappeared behind our back (a chain reset, say).
    fn sync(&mut self, len: usize) {
        while self.keys.len() < len {
            self.keys.push(self.next);
            self.next += 1;
        }
        self.keys.truncate(len);
    }

    fn at(&self, idx: usize) -> u64 {
        self.keys.get(idx).copied().unwrap_or_default()
    }

    fn move_step(&mut self, from: usize, to: usize) {
        let key = self.keys.remove(from);
        self.keys.insert(to, key);
    }

    fn forget(&mut self, idx: usize) {
        self.keys.remove(idx);
    }
}

/// How far a step still is from the slot it now occupies, so reorders settle
/// instead of snapping.
///
/// The glide is armed only while a step is actually being dragged. Otherwise the
/// row tracks its slot exactly: a modal's first frame lays out before it knows
/// its own size, and animating that correction would slide the whole ladder into
/// place every time the modal opens.
fn animated_step_offset(
    ui: &egui::Ui,
    section: &'static str,
    key: u64,
    dragging: bool,
) -> egui::Vec2 {
    let target_y = ui.next_widget_position().y;
    let animation_id = egui::Id::new((section, "step-pos", key));
    let seconds = if dragging { STEP_SETTLE_SECONDS } else { 0.0 };
    let animated_y = ui
        .ctx()
        .animate_value_with_time(animation_id, target_y, seconds);
    egui::vec2(0.0, animated_y - target_y)
}

/// Grip that lifts its step onto the cursor, in the slot the up/down buttons
/// used to share. Always fully visible: this ladder is short and already dense
/// with controls, so the preset list's proximity reveal would only add flicker.
fn drag_grip(ui: &mut egui::Ui, id: egui::Id, lifting: bool) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(crate::gui::icons::ICON_MD, STEP_ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let response = ui.interact(rect, id, egui::Sense::drag());
    let color = if response.hovered() || response.dragged() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    crate::gui::icons::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(crate::gui::icons::ICON_MD)),
        Icon::DragIndicator,
        color,
    );
    response.on_hover_cursor(if lifting {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    })
}

/// The hole the carried step will drop into, drawn as an outline so the ladder
/// visibly parts rather than silently resorting itself.
fn landing_gap(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    reorder: &ListReorder,
    tinted: bool,
) -> egui::Rect {
    let fallback = egui::vec2(ui.available_width(), STEP_ROW_HEIGHT + 6.0);
    let (rect, _) = ui.allocate_exact_size(reorder.slot_size(fallback), egui::Sense::hover());
    let fill = if tinted {
        blend(theme.card_bg(), theme.on_surface(), 0.04)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        CornerRadius::same(8),
        theme.card_stroke(),
        egui::StrokeKind::Inside,
    );
    rect
}

/// The carried step itself: a copy that tracks the cursor above the ladder,
/// lifted off the surface with a shadow. It is deliberately inert — the live
/// widgets stay with the row in the list, which keeps their ids unique.
fn draw_lifted_step(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    reorder: &ListReorder,
    section: &'static str,
    chain: &[String],
    ui_language: &str,
) {
    let Some(floating) = reorder.floating(ui) else {
        return;
    };
    let Some(model_id) = chain.get(floating.from) else {
        return;
    };
    let label = model_selector::model_short_label(model_id, ui_language);
    let step_number = floating.insert + 2;

    // Tooltip order, not Foreground: the ladder lives inside an `egui::Modal`,
    // which is itself a Foreground area. Sharing that tier leaves the paint order
    // down to which area egui saw first, so the copy would sometimes come up
    // behind the dialog surface. This is the layer egui's own drag preview uses.
    egui::Area::new(egui::Id::new((section, "lifted-step")))
        .order(egui::Order::Tooltip)
        .fixed_pos(floating.origin)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            ui.set_width(floating.size.x);
            egui::Frame::new()
                .fill(blend(theme.card_bg(), theme.on_surface(), 0.10))
                .stroke(theme.card_stroke())
                .corner_radius(CornerRadius::same(8))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 16,
                    spread: 0,
                    color: theme.scrim_color(),
                })
                .inner_margin(Margin::symmetric(
                    crate::gui::theme::space::SNUG,
                    crate::gui::theme::space::TIGHT,
                ))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(STEP_ROW_HEIGHT);
                        ui.add_sized(
                            egui::vec2(STEP_NUMBER_WIDTH, STEP_ROW_HEIGHT),
                            egui::Label::new(
                                egui::RichText::new(format!("{step_number}"))
                                    .size(11.5)
                                    .color(theme.on_surface_variant()),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(label)
                                .size(12.5)
                                .color(theme.on_surface()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            crate::gui::icons::draw_icon_static(
                                ui,
                                Icon::DragIndicator,
                                Some(crate::gui::icons::ICON_MD),
                            );
                        });
                    });
                });
        });
}

/// Reorder/remove control that renders dimmed and inert at the chain's edges
/// instead of looking clickable while doing nothing.
fn reorder_clicked(ui: &mut egui::Ui, icon: Icon, enabled: bool) -> bool {
    let opacity = if enabled { 1.0 } else { 0.25 };
    let response = crate::gui::icons::icon_button_sized_with_opacity(
        ui,
        icon,
        crate::gui::icons::ICON_MD,
        opacity,
    );
    if !enabled {
        return false;
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_keys_follow_their_step_through_reorders_and_removals() {
        let mut keys = StepKeys::default();
        keys.sync(3);
        let original = keys.keys.clone();
        assert_eq!(original.len(), 3, "one key per step");

        // Dragging the last step to the front must carry its key along, or the
        // in-flight drag loses the widget it started on.
        keys.move_step(2, 0);
        assert_eq!(keys.at(0), original[2]);
        assert_eq!(keys.at(1), original[0]);

        keys.forget(1);
        assert_eq!(keys.at(0), original[2]);
        assert!(!keys.keys.contains(&original[0]));

        // A step added afterwards must not reuse a key that is still animating.
        keys.sync(3);
        assert!(!original.contains(&keys.at(2)));
    }

    #[test]
    fn step_keys_shrink_when_a_chain_is_replaced_wholesale() {
        let mut keys = StepKeys::default();
        keys.sync(4);
        keys.sync(2);
        assert_eq!(keys.keys.len(), 2);
        keys.sync(3);
        assert_eq!(keys.keys.len(), 3);
        assert_ne!(keys.at(2), keys.at(0), "keys must stay unique");
    }
}
