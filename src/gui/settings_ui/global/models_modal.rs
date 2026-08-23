//! Shared "Models" hub modal: one Material surface hosting the model priority
//! chains, live usage statistics, and custom model management as three tabs.
//!
//! Every tab renders inside the same shell (title, supporting copy, segmented
//! tab bar, height-capped body) so the tabs share one width, one header, and
//! one scroll contract.

use crate::config::Config;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::usage_stats::UsageStore;
use eframe::egui::{self, Color32};

use super::custom_models::render_custom_models_body;
use super::model_priority::render_model_priority_body;
use super::usage_stats::{render_usage_body, usage_descriptions, usage_dialog_layout};

/// Header title size; the tab strip and description are baseline-matched to it.
const TITLE_FONT_SIZE: f32 = crate::gui::widgets::DIALOG_TITLE_SIZE;

/// Hold per description line — longer than the table's cell rotation, since a
/// full sentence takes longer to read than `RPD 999/1000`.
const DESCRIPTION_HOLD_SECONDS: f64 = 2.0;

/// Tabs of the Models hub, in presentation order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelsTab {
    #[default]
    Priority,
    Usage,
    Custom,
}

impl ModelsTab {
    const ALL: [Self; 3] = [Self::Priority, Self::Usage, Self::Custom];

    fn label(self, text: &LocaleText) -> &'static str {
        match self {
            Self::Priority => text.model_catalog.models_hub_tab_priority,
            Self::Usage => text.model_catalog.models_hub_tab_usage,
            Self::Custom => text.model_catalog.models_hub_tab_custom,
        }
    }

    fn icon(self) -> crate::gui::icons::Icon {
        match self {
            Self::Priority => crate::gui::icons::Icon::Priority,
            Self::Usage => crate::gui::icons::Icon::BarChart,
            Self::Custom => crate::gui::icons::Icon::Settings,
        }
    }

    fn accent(self, theme: &AppTheme) -> Color32 {
        match self {
            Self::Priority => theme.btn_priority(),
            Self::Usage => theme.btn_stats(),
            Self::Custom => theme.btn_tools(),
        }
    }

    /// Supporting copy for this tab. More than one line means the header
    /// crossfades between them.
    fn descriptions(self, usage_stats: &UsageStore, text: &LocaleText) -> Vec<&'static str> {
        match self {
            Self::Priority => vec![text.model_catalog.model_priority_skip_hint],
            Self::Usage => usage_descriptions(usage_stats, text),
            Self::Custom => vec![text.model_catalog.custom_models_desc],
        }
    }
}

/// State the hub needs from the enclosing app, grouped to keep the call short.
pub struct ModelsHubState<'a> {
    pub show_modal: &'a mut bool,
    pub tab: &'a mut ModelsTab,
}

/// Provider toggles that decide which usage rows are live.
#[derive(Clone, Copy)]
pub struct ProviderEnabled {
    pub groq: bool,
    pub gemini: bool,
    pub openrouter: bool,
    pub nvidia: bool,
    pub ollama: bool,
}

/// Renders the Models hub modal. Returns `true` when config changed.
pub fn render_models_modal(
    ui: &mut egui::Ui,
    config: &mut Config,
    usage_stats: &UsageStore,
    text: &LocaleText,
    providers: ProviderEnabled,
    state: ModelsHubState<'_>,
) -> bool {
    let ModelsHubState { show_modal, tab } = state;
    if !*show_modal {
        return false;
    }

    let theme = AppTheme::from_ui(ui);
    let layout = usage_dialog_layout(ui.ctx().content_rect().size());
    let mut changed = false;

    let modal = crate::gui::widgets::material_modal(
        ui.ctx(),
        &theme,
        egui::Id::new("models_hub_modal"),
        |ui| {
            ui.set_width(layout.width);

            render_header(ui, &theme, text, usage_stats, tab, show_modal);
            ui.add_space(10.0);

            // The body caps at the viewport-derived height (the scrolling tabs
            // fill it) but shorter tabs shrink the dialog instead of leaving a
            // half-empty surface.
            let body_height = layout.body_height;
            ui.scope(|ui| {
                ui.set_max_height(body_height);
                match *tab {
                    ModelsTab::Priority => {
                        // Scrolls only once a chain outgrows the cap; short
                        // chains keep the dialog compact.
                        egui::ScrollArea::vertical()
                            .id_salt("model_priority_body")
                            .max_height(body_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if render_model_priority_body(ui, config, text) {
                                    changed = true;
                                }
                            });
                    }
                    ModelsTab::Usage => render_usage_body(
                        ui,
                        usage_stats,
                        text,
                        &config.ui_language,
                        providers.groq,
                        providers.gemini,
                        providers.openrouter,
                        providers.nvidia,
                        providers.ollama,
                        &config.custom_models,
                        body_height,
                    ),
                    ModelsTab::Custom => {
                        if render_custom_models_body(ui, config, text, body_height) {
                            changed = true;
                        }
                    }
                }
            });
        },
    );

    if modal.should_close() {
        *show_modal = false;
    }

    changed
}

/// Single-row header: dialog name, tab strip, active tab's supporting copy,
/// close. Keeping all four on one line buys the body ~40px of height.
fn render_header(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    text: &LocaleText,
    usage_stats: &UsageStore,
    tab: &mut ModelsTab,
    show_modal: &mut bool,
) {
    let descriptions = tab.descriptions(usage_stats, text);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text.model_catalog.models_hub_button)
                .size(TITLE_FONT_SIZE)
                .strong()
                .color(theme.on_surface()),
        );
        ui.add_space(10.0);
        render_tab_bar(ui, theme, text, tab);

        // Close is pinned right; the description fills whatever is left and
        // truncates rather than pushing the layout.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::gui::icons::icon_button(ui, crate::gui::icons::Icon::Close).clicked() {
                *show_modal = false;
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (index, opacity) = crate::gui::widgets::crossfade_phase_held(
                    ui,
                    descriptions.len(),
                    DESCRIPTION_HOLD_SECONDS,
                );
                let description = descriptions[index];
                crate::gui::widgets::baseline_aligned(
                    ui,
                    TITLE_FONT_SIZE,
                    crate::gui::widgets::DIALOG_DESCRIPTION_SIZE,
                    |ui| {
                        // egui shows the full text itself once the label
                        // elides; an explicit tooltip would stack a second.
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(description)
                                    .size(crate::gui::widgets::DIALOG_DESCRIPTION_SIZE)
                                    .color(theme.on_surface_variant().gamma_multiply(opacity)),
                            )
                            .truncate(),
                        );
                    },
                );
            });
        });
    });
}

/// Segmented tab strip: the active tab is an accent-filled pill, the inactive
/// ones a quiet neutral pill — same filled-button language as the rest of the
/// settings surface.
fn render_tab_bar(ui: &mut egui::Ui, theme: &AppTheme, text: &LocaleText, tab: &mut ModelsTab) {
    ui.horizontal(|ui| {
        for candidate in ModelsTab::ALL {
            let selected = candidate == *tab;
            let (fill, label_color) = if selected {
                (candidate.accent(theme), theme.on_accent())
            } else {
                (theme.neutral_fill(), theme.on_surface_variant())
            };

            if crate::gui::widgets::filled_icon_button(
                ui,
                candidate.icon(),
                candidate.label(text),
                fill,
                label_color,
                10,
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            {
                *tab = candidate;
            }
            ui.add_space(8.0);
        }
    });
}
