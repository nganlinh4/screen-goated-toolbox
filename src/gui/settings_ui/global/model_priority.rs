use crate::config::types::LiveModelOverrides;
use crate::config::{Config, ModelPriorityChains};
use crate::gui::icons::Icon;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::list_reorder::{ListReorder, Slot};
use crate::gui::settings_ui::model_selector;
use crate::gui::theme::{AppTheme, blend};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui::{self, Color32, CornerRadius, Margin};

#[path = "model_priority/feed.rs"]
mod feed;
#[path = "model_priority/step_keys.rs"]
mod step_keys;

use step_keys::StepKeys;

const STEP_ROW_HEIGHT: f32 = 24.0;
const STEP_NUMBER_WIDTH: f32 = 16.0;

struct ChainEditorState<'a> {
    authored: &'a mut Vec<String>,
    adaptive_enabled: &'a mut bool,
    overrides: &'a mut LiveModelOverrides,
}

/// Renders the Model Priority tab body (chain columns only).
///
/// Modal chrome, title, and description are owned by the shared models hub.
pub fn render_model_priority_body(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let ui_language = config.ui_language.clone();
    let mut image_adaptive = config.adaptive_model_priority.image_to_text;
    let mut text_adaptive = config.adaptive_model_priority.text_to_text;
    let mut image_overrides = config
        .adaptive_model_priority
        .image_to_text_overrides
        .clone();
    let mut text_overrides = config
        .adaptive_model_priority
        .text_to_text_overrides
        .clone();
    let selector_models = model_selector::selector_models(&config.custom_models);
    let image_prepared = feed::prepare_chain(
        config,
        RetryChainKind::ImageToText,
        &config.model_priority_chains.image_to_text,
        &image_overrides,
        image_adaptive,
    );
    let text_prepared = feed::prepare_chain(
        config,
        RetryChainKind::TextToText,
        &config.model_priority_chains.text_to_text,
        &text_overrides,
        text_adaptive,
    );
    ui.columns(2, |columns| {
        if render_chain_section(
            &mut columns[0],
            ChainEditorState {
                authored: &mut config.model_priority_chains.image_to_text,
                adaptive_enabled: &mut image_adaptive,
                overrides: &mut image_overrides,
            },
            RetryChainKind::ImageToText,
            image_prepared,
            &selector_models,
            &ui_language,
            text,
        ) {
            changed = true;
        }

        if render_chain_section(
            &mut columns[1],
            ChainEditorState {
                authored: &mut config.model_priority_chains.text_to_text,
                adaptive_enabled: &mut text_adaptive,
                overrides: &mut text_overrides,
            },
            RetryChainKind::TextToText,
            text_prepared,
            &selector_models,
            &ui_language,
            text,
        ) {
            changed = true;
        }
    });
    if config.adaptive_model_priority.image_to_text != image_adaptive
        || config.adaptive_model_priority.text_to_text != text_adaptive
    {
        config.adaptive_model_priority.image_to_text = image_adaptive;
        config.adaptive_model_priority.text_to_text = text_adaptive;
        changed = true;
    }
    if config.adaptive_model_priority.image_to_text_overrides != image_overrides
        || config.adaptive_model_priority.text_to_text_overrides != text_overrides
    {
        config.adaptive_model_priority.image_to_text_overrides = image_overrides;
        config.adaptive_model_priority.text_to_text_overrides = text_overrides;
        changed = true;
    }
    changed
}

/// One retry chain rendered as a numbered ladder.
fn render_chain_section(
    ui: &mut egui::Ui,
    state: ChainEditorState<'_>,
    chain_kind: RetryChainKind,
    prepared: feed::PreparedChain,
    selector_models: &[crate::model_config::ModelConfig],
    ui_language: &str,
    text: &LocaleText,
) -> bool {
    let ChainEditorState {
        authored: chain,
        adaptive_enabled,
        overrides,
    } = state;
    let feed::PreparedChain {
        mut visible,
        adaptive,
        live_ids,
    } = prepared;
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
                let was_adaptive = *adaptive_enabled;
                if ui
                    .toggle_value(
                        adaptive_enabled,
                        text.model_catalog.model_priority_live_toggle,
                    )
                    .on_hover_text(text.model_catalog.model_priority_live_toggle_hint)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .changed()
                {
                    if was_adaptive && !*adaptive_enabled {
                        chain.clone_from(&visible);
                    } else if !was_adaptive && *adaptive_enabled {
                        visible.clone_from(&adaptive);
                    }
                    changed = true;
                }
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
                        *adaptive_enabled = true;
                        *overrides = LiveModelOverrides::default();
                        changed = true;
                    }
                });
            });
            ui.add_space(6.0);

            // Step 0: the user's explicit pick, locked ahead of every retry row.
            step_row(ui, &theme, 0, false, |ui| {
                fixed_step_label(
                    ui,
                    &theme,
                    text.model_catalog.model_priority_chosen_model,
                    text.model_catalog.model_priority_fixed_hint,
                );
            });

            let keys_id = egui::Id::new((section_id, "step-keys"));
            let mut keys: StepKeys = ui.data(|data| data.get_temp(keys_id)).unwrap_or_default();
            let mut visible_chain = visible;
            keys.sync(visible_chain.len());
            let mut reorder = ListReorder::load(ui, section_id);
            reorder.track(ui, visible_chain.len());
            let lifting = reorder.is_lifting();
            let visible_count = visible_chain.len();

            let mut removal: Option<usize> = None;
            let mut lift_request: Option<(usize, egui::Rect)> = None;
            let mut manual_edits = Vec::new();
            for slot in reorder.plan(visible_count) {
                let chain_idx = match slot {
                    Slot::Gap => {
                        reorder.note_slot(landing_gap(ui, &theme, &reorder, false));
                        continue;
                    }
                    Slot::Step(idx) => idx,
                };
                let step_number = chain_idx + 1;
                let tinted = (step_number - 1).is_multiple_of(2);

                let key = keys.at(chain_idx);
                let mut remove_clicked = false;
                let mut grip = None;
                let row_rect = step_row(ui, &theme, step_number, tinted, |ui| {
                    let old_model = visible_chain[chain_idx].clone();
                    if model_selector::render_model_combo_from_models(
                        ui,
                        (section_id, "combo", key),
                        &mut visible_chain[chain_idx],
                        chain_kind,
                        ui_language,
                        selector_models,
                    ) {
                        manual_edits.push(feed::ManualEdit::Replace {
                            old: old_model,
                            new: visible_chain[chain_idx].clone(),
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if reorder_clicked(ui, Icon::Close, true) {
                            remove_clicked = true;
                        }
                        grip = Some(drag_grip(
                            ui,
                            egui::Id::new((section_id, "chain-grip", key)),
                            lifting,
                        ));
                    });
                });

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
            draw_lifted_step(
                ui,
                &theme,
                &reorder,
                section_id,
                &visible_chain,
                selector_models,
                ui_language,
            );

            // Mutations land after the pass: rewriting a chain the loop is
            // still walking would skip or repeat a step.
            if let Some(idx) = removal {
                let removed = visible_chain.remove(idx);
                keys.forget(idx);
                manual_edits.push(feed::ManualEdit::Remove(removed));
            } else if let Some((from, to)) = reorder.settle(ui) {
                let step = visible_chain.remove(from);
                manual_edits.push(feed::ManualEdit::Move(step.clone()));
                visible_chain.insert(to, step);
                keys.move_step(from, to);
            }

            // Tail step: everything else, in smart fallback order.
            step_row(ui, &theme, visible_chain.len() + 1, false, |ui| {
                fixed_step_label(
                    ui,
                    &theme,
                    text.model_catalog.model_priority_auto,
                    text.model_catalog.model_priority_auto_hint,
                );
            });

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
                let added =
                    model_selector::default_model_id_from_models(selector_models, chain_kind);
                visible_chain.push(added.clone());
                keys.sync(visible_chain.len());
                manual_edits.push(feed::ManualEdit::Add(added));
            }

            if !manual_edits.is_empty() {
                if *adaptive_enabled {
                    *adaptive_enabled &= feed::commit_manual_edits(
                        chain,
                        visible_chain,
                        overrides,
                        &live_ids,
                        &manual_edits,
                    );
                } else {
                    *chain = visible_chain;
                }
                changed = true;
            }
            ui.data_mut(|data| data.insert_temp(keys_id, keys));
            reorder.store(ui);
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
                        egui::RichText::new(step.to_string())
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
    selector_models: &[crate::model_config::ModelConfig],
    ui_language: &str,
) {
    let Some(floating) = reorder.floating(ui) else {
        return;
    };
    let Some(model_id) = chain.get(floating.from) else {
        return;
    };
    let label =
        model_selector::model_short_label_from_models(selector_models, model_id, ui_language);
    let step_number = floating.insert + 1;

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

    #[test]
    fn centered_priority_modal_never_uses_visual_row_transforms() {
        let source = include_str!("model_priority.rs");
        assert!(!source.contains(&["with_visual", "_transform"].concat()));
        assert!(!source.contains(&["animated_step", "_offset"].concat()));
    }
}
