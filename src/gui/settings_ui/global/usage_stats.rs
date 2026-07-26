use crate::config::types::CustomModelDefinition;
use crate::gui::locale::LocaleText;
use crate::gui::theme::{AppTheme, blend};
use crate::model_config::{ModelConfig, get_all_models_with_custom};
use crate::usage_stats::{
    UsageFreshness, UsageKey, UsageSnapshot, UsageStore, endpoint_representatives, freshness_at,
    now_unix_seconds,
};
use eframe::egui;
use std::collections::BTreeMap;

#[path = "usage_stats_table.rs"]
mod usage_stats_table;

use usage_stats_table::{cell_ui, endpoint_columns, provider_header_rects, render_status_strip};

const DIALOG_HORIZONTAL_MARGIN: f32 = 32.0;
const DIALOG_VERTICAL_RESERVE: f32 = 64.0;
const DIALOG_MAX_WIDTH: f32 = 1170.0;
const DIALOG_MAX_BODY_HEIGHT: f32 = 570.0;
const WIDE_DIALOG_MIN_WIDTH: f32 = 900.0;
const WIDE_COLUMN_COUNT: usize = 2;
const PROVIDER_HEADER_HEIGHT: f32 = 20.0;
const ENDPOINT_ROW_HEIGHT: f32 = 22.0;
const ENDPOINT_NAME_FONT_SIZE: f32 = 11.5;
const ENDPOINT_ID_FONT_SIZE: f32 = 9.5;
const ENDPOINT_STATUS_FONT_SIZE: f32 = 10.5;

#[derive(Clone, Copy)]
struct ProviderToggles {
    groq: bool,
    gemini: bool,
    openrouter: bool,
    ollama: bool,
    cerebras: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UsageDialogLayout {
    width: f32,
    body_height: f32,
    column_count: usize,
}

type ProviderSectionRows<'a> = (String, Vec<&'a ModelConfig>);

#[expect(
    clippy::too_many_arguments,
    reason = "modal rendering consumes distinct provider toggles and shared UI state"
)]
pub fn render_usage_modal(
    ui: &mut egui::Ui,
    usage_stats: &UsageStore,
    text: &LocaleText,
    lang: &str,
    show_modal: &mut bool,
    use_groq: bool,
    use_gemini: bool,
    use_openrouter: bool,
    use_ollama: bool,
    use_cerebras: bool,
    custom_models: &[CustomModelDefinition],
) {
    if !*show_modal {
        return;
    }

    let theme = AppTheme::from_ui(ui);
    let content_rect = ui.ctx().content_rect();
    let layout = usage_dialog_layout(content_rect.size());
    let toggles = ProviderToggles {
        groq: use_groq,
        gemini: use_gemini,
        openrouter: use_openrouter,
        ollama: use_ollama,
        cerebras: use_cerebras,
    };

    let modal = egui::Modal::new(egui::Id::new("usage_statistics_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(layout.width);
            let description = if usage_stats.is_empty() {
                text.desktop_settings.usage_no_live_data
            } else {
                text.desktop_settings.usage_session_hint
            };

            if crate::gui::widgets::dialog_header(
                ui,
                &theme,
                text.desktop_settings.usage_statistics_title,
                Some(description),
                |_| {},
            ) {
                *show_modal = false;
            }

            let all_models = get_all_models_with_custom(custom_models);
            let rows = endpoint_representatives(&all_models);
            let sections = group_rows(rows, toggles);

            egui::ScrollArea::vertical()
                .max_height(layout.body_height)
                .min_scrolled_height(layout.body_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(layout.body_height);
                    let columns = balance_sections(sections, layout.column_count);
                    ui.columns(layout.column_count, |column_uis| {
                        for (column_ui, column_sections) in
                            column_uis.iter_mut().zip(columns.iter())
                        {
                            column_ui.spacing_mut().item_spacing.y = 6.0;
                            for (section_key, section_rows) in column_sections {
                                render_provider_section(
                                    column_ui,
                                    &theme,
                                    text,
                                    lang,
                                    section_key,
                                    section_rows,
                                    usage_stats,
                                );
                            }
                        }
                    });
                });
        });

    if modal.should_close() {
        *show_modal = false;
    }
}

fn usage_dialog_layout(viewport_size: egui::Vec2) -> UsageDialogLayout {
    let width = (viewport_size.x - DIALOG_HORIZONTAL_MARGIN).clamp(360.0, DIALOG_MAX_WIDTH);
    let body_height =
        (viewport_size.y - DIALOG_VERTICAL_RESERVE).clamp(280.0, DIALOG_MAX_BODY_HEIGHT);
    UsageDialogLayout {
        width,
        body_height,
        column_count: if width >= WIDE_DIALOG_MIN_WIDTH {
            WIDE_COLUMN_COUNT
        } else {
            1
        },
    }
}

fn group_rows(
    rows: Vec<&ModelConfig>,
    toggles: ProviderToggles,
) -> BTreeMap<String, Vec<&ModelConfig>> {
    let mut sections: BTreeMap<String, Vec<&ModelConfig>> = BTreeMap::new();
    for model in rows {
        if !provider_enabled(&model.provider, toggles) {
            continue;
        }
        sections
            .entry(provider_group(&model.provider).to_string())
            .or_default()
            .push(model);
    }
    sections
}

fn balance_sections<'a>(
    sections: BTreeMap<String, Vec<&'a ModelConfig>>,
    column_count: usize,
) -> Vec<Vec<ProviderSectionRows<'a>>> {
    let column_count = column_count.max(1);
    let mut ordered: Vec<_> = sections.into_iter().collect();
    ordered.sort_by(|left, right| {
        section_weight(&right.0, right.1.len())
            .cmp(&section_weight(&left.0, left.1.len()))
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut columns: Vec<Vec<ProviderSectionRows<'a>>> =
        (0..column_count).map(|_| Vec::new()).collect();
    let mut weights = vec![0_usize; column_count];
    for section in ordered {
        let target = weights
            .iter()
            .enumerate()
            .min_by_key(|(index, weight)| (**weight, *index))
            .map(|(index, _)| index)
            .unwrap_or(0);
        weights[target] += section_weight(&section.0, section.1.len());
        columns[target].push(section);
    }
    columns
}

fn section_weight(section_key: &str, endpoint_count: usize) -> usize {
    endpoint_count + 2 + usize::from(section_key == "openrouter")
}

fn provider_enabled(provider: &str, toggles: ProviderToggles) -> bool {
    match provider {
        "groq" => toggles.groq,
        "google" | "gemini-live" => toggles.gemini,
        "openrouter" => toggles.openrouter,
        "ollama" => toggles.ollama,
        "cerebras" => toggles.cerebras,
        _ => true,
    }
}

fn provider_group(provider: &str) -> &str {
    match provider {
        "google" | "gemini-live" => "google",
        other => other,
    }
}

fn render_provider_section(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    text: &LocaleText,
    lang: &str,
    section_key: &str,
    rows: &[&ModelConfig],
    usage_stats: &UsageStore,
) {
    let accent = provider_accent(theme, section_key);
    egui::Frame::new()
        .fill(blend(theme.dialog_surface(), accent, 0.035))
        .stroke(egui::Stroke::new(
            1.0,
            blend(theme.card_stroke().color, accent, 0.24),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            render_section_header(ui, text, section_key, accent);

            if section_key == "openrouter" {
                let shared_quota = rows
                    .first()
                    .map(|model| model.localized_quota(lang))
                    .filter(|quota| !quota.is_empty())
                    .unwrap_or("—");
                let mut status = shared_quota.to_string();
                let mut color = accent;
                if let Some(snapshot) = usage_stats.get(&UsageKey::provider("openrouter")) {
                    let (snapshot_label, snapshot_color) = snapshot_summary(snapshot, text, theme);
                    status.push_str(" · ");
                    status.push_str(&snapshot_label);
                    color = snapshot_color;
                }
                render_status_strip(
                    ui,
                    text.desktop_settings.usage_shared_quota,
                    status,
                    color,
                    theme,
                    ENDPOINT_ROW_HEIGHT,
                    ENDPOINT_STATUS_FONT_SIZE,
                );
            }

            for model in rows {
                render_endpoint_row(
                    ui,
                    theme,
                    text,
                    lang,
                    model,
                    usage_stats,
                    section_key == "openrouter",
                );
            }
        });
}

fn render_section_header(
    ui: &mut egui::Ui,
    text: &LocaleText,
    section_key: &str,
    accent: egui::Color32,
) {
    let row_width = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(
        egui::vec2(row_width, PROVIDER_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let columns = provider_header_rects(row_rect);

    let mut icon_ui = cell_ui(
        ui,
        columns.icon,
        egui::Layout::left_to_right(egui::Align::Center),
    );
    crate::gui::icons::draw_icon_static(
        &mut icon_ui,
        crate::gui::icons::provider_icon(section_key),
        Some(crate::gui::icons::ICON_SM),
    );

    let provider_name = provider_name(section_key);
    let mut name_ui = cell_ui(
        ui,
        columns.name,
        egui::Layout::left_to_right(egui::Align::Center),
    );
    name_ui
        .add(egui::Label::new(egui::RichText::new(&provider_name).strong().size(12.0)).truncate())
        .on_hover_text(&provider_name);

    if let Some(url) = provider_dashboard(section_key) {
        let mut link_ui = cell_ui(
            ui,
            columns.link,
            egui::Layout::right_to_left(egui::Align::Center),
        );
        let response = link_ui
            .add(
                egui::Button::new(
                    egui::RichText::new(text.desktop_settings.usage_check_link)
                        .size(9.5)
                        .color(accent),
                )
                .frame(false),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
}

fn render_endpoint_row(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    text: &LocaleText,
    lang: &str,
    model: &ModelConfig,
    usage_stats: &UsageStore,
    provider_quota_is_shared: bool,
) {
    egui::Frame::new()
        .fill(blend(theme.dialog_surface(), theme.neutral_fill(), 0.16))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row_width = ui.available_width();
            let columns = endpoint_columns(row_width);
            let snapshot = usage_stats.get(&UsageKey::endpoint(&model.provider, &model.full_name));
            let (status, status_color) =
                endpoint_status(snapshot, model, provider_quota_is_shared, text, lang, theme);
            let (row_rect, row_response) = ui.allocate_exact_size(
                egui::vec2(row_width, ENDPOINT_ROW_HEIGHT),
                egui::Sense::hover(),
            );
            let rects = columns.rects(row_rect);

            let mut prefix_ui = cell_ui(
                ui,
                rects.prefix,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            crate::gui::model_performance::render_prefix(&mut prefix_ui, model);

            let localized_name = model.localized_name(lang);
            let mut name_ui = cell_ui(
                ui,
                rects.name,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            name_ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(localized_name)
                            .size(ENDPOINT_NAME_FONT_SIZE)
                            .strong(),
                    )
                    .truncate(),
                )
                .on_hover_text(localized_name);

            let mut id_ui = cell_ui(
                ui,
                rects.id,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            id_ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(format!("· {}", model.full_name))
                            .monospace()
                            .size(ENDPOINT_ID_FONT_SIZE)
                            .color(theme.on_surface_variant()),
                    )
                    .truncate(),
                )
                .on_hover_text(&model.full_name);

            if !status.is_empty() {
                let mut status_ui = cell_ui(
                    ui,
                    rects.status,
                    egui::Layout::left_to_right(egui::Align::Center),
                );
                status_ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(&status)
                                .monospace()
                                .size(ENDPOINT_STATUS_FONT_SIZE)
                                .color(status_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&status);
            }

            row_response.on_hover_text(format!("{} · {}", model.provider, model.full_name));
        });
}

fn endpoint_status(
    snapshot: Option<&UsageSnapshot>,
    model: &ModelConfig,
    provider_quota_is_shared: bool,
    text: &LocaleText,
    lang: &str,
    theme: &AppTheme,
) -> (String, egui::Color32) {
    match snapshot {
        Some(snapshot) => snapshot_summary(snapshot, text, theme),
        None if provider_quota_is_shared => (String::new(), theme.on_surface_variant()),
        None => (
            model.localized_quota(lang).to_string(),
            theme.on_surface_variant(),
        ),
    }
}

fn snapshot_summary(
    snapshot: &UsageSnapshot,
    text: &LocaleText,
    theme: &AppTheme,
) -> (String, egui::Color32) {
    let mut segments = Vec::with_capacity(snapshot.metrics.len() + 1);
    for metric in &snapshot.metrics {
        let remaining = metric.remaining.as_deref().unwrap_or("—");
        let limit = metric.limit.as_deref().unwrap_or("—");
        let mut label = format!("{} {remaining}/{limit}", metric.kind.label());
        if let Some(reset) = &metric.reset {
            label.push_str(&format!(" ↻{reset}"));
        }
        segments.push(label);
    }

    let now = now_unix_seconds();
    let minutes = now
        .saturating_sub(snapshot.observed_at_unix_seconds)
        .div_ceil(60);
    let (freshness, color) = match freshness_at(snapshot.observed_at_unix_seconds, now) {
        UsageFreshness::Fresh => (
            text.desktop_settings.usage_updated_now.to_string(),
            theme.success(),
        ),
        UsageFreshness::Aging => (
            format!("{minutes} {}", text.desktop_settings.usage_minutes_ago),
            theme.warning(),
        ),
        UsageFreshness::Stale => (
            format!(
                "{} · {minutes} {}",
                text.desktop_settings.usage_stale, text.desktop_settings.usage_minutes_ago
            ),
            theme.danger_text(),
        ),
    };
    segments.push(freshness);
    (segments.join(" · "), color)
}

fn provider_name(provider: &str) -> String {
    match provider {
        "google" => "Google Gemini".to_string(),
        "google-gtx" => "Google Translate".to_string(),
        "groq" => "Groq".to_string(),
        "cerebras" => "Cerebras".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        "ollama" => "Ollama".to_string(),
        "qrserver" => "QR Server".to_string(),
        "parakeet" => "Parakeet".to_string(),
        "qwen3" => "Qwen Local".to_string(),
        "taalas" => "Taalas".to_string(),
        _ => provider.to_string(),
    }
}

fn provider_dashboard(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("https://console.groq.com/docs/rate-limits"),
        "cerebras" => Some("https://cloud.cerebras.ai/"),
        "google" => Some("https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit"),
        "openrouter" => Some("https://openrouter.ai/activity"),
        _ => None,
    }
}

fn provider_accent(theme: &AppTheme, provider: &str) -> egui::Color32 {
    match provider {
        "groq" => theme.warning(),
        "cerebras" => theme.danger_text(),
        "google" => theme.accent_help(),
        "openrouter" => theme.accent_fill(),
        "taalas" => theme.accent_three_d_generator(),
        _ => theme.on_surface_variant(),
    }
}

#[cfg(test)]
#[path = "usage_stats_tests.rs"]
mod tests;
