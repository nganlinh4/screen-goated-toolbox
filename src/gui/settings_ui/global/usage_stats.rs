use super::models_modal::ProviderEnabled;
use crate::config::types::CustomModelDefinition;
use crate::gui::locale::LocaleText;
use crate::gui::theme::{AppTheme, blend};
use crate::model_config::{ModelConfig, get_all_models_with_custom};
use crate::usage_stats::{
    UsageFreshness, UsageKey, UsageMetric, UsageSnapshot, UsageStore, endpoint_representatives,
    freshness_at, now_unix_seconds,
};
use eframe::egui;
use std::collections::BTreeMap;
use std::time::Duration;

#[path = "usage_stats_table.rs"]
mod usage_stats_table;

use usage_stats_table::{cell_ui, endpoint_columns, provider_header_rects, render_status_strip};

const DIALOG_HORIZONTAL_MARGIN: f32 = 32.0;
const DIALOG_VERTICAL_RESERVE: f32 = 64.0;
/// Cap for the models hub dialog. Every tab reads comfortably below this and
/// it keeps the surface clear of the window edges.
const DIALOG_MAX_WIDTH: f32 = 1000.0;
const DIALOG_MAX_BODY_HEIGHT: f32 = 570.0;
const WIDE_DIALOG_MIN_WIDTH: f32 = 900.0;
const WIDE_COLUMN_COUNT: usize = 2;
const PROVIDER_HEADER_HEIGHT: f32 = 20.0;
const ENDPOINT_ROW_HEIGHT: f32 = 22.0;
const ENDPOINT_NAME_FONT_SIZE: f32 = 11.5;
const ENDPOINT_ID_FONT_SIZE: f32 = 9.5;
const ENDPOINT_STATUS_FONT_SIZE: f32 = 10.5;
const METRIC_HOLD_SECONDS: f64 = 3.0;
const LOW_QUOTA_RATIO: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsageDialogLayout {
    pub width: f32,
    pub body_height: f32,
    pub column_count: usize,
}

type ProviderSectionRows<'a> = (String, Vec<&'a ModelConfig>);

/// Renders the Usage Statistics tab body inside the shared models hub.
///
/// `layout` is the hub's own layout: its fixed body height keeps tab switches
/// from resizing the modal, and its column count is the single source shared
/// with the parity fixture.
pub fn render_usage_body(
    ui: &mut egui::Ui,
    usage_stats: &UsageStore,
    text: &LocaleText,
    lang: &str,
    providers: ProviderEnabled,
    custom_models: &[CustomModelDefinition],
    layout: UsageDialogLayout,
) {
    let theme = AppTheme::from_ui(ui);
    if !usage_stats.is_empty() {
        ui.ctx().request_repaint_after(Duration::from_secs(30));
    }
    let UsageDialogLayout {
        body_height,
        column_count,
        ..
    } = layout;

    let all_models = get_all_models_with_custom(custom_models);
    let rows = endpoint_representatives(&all_models);
    let sections = group_rows(rows, providers);

    egui::ScrollArea::vertical()
        .id_salt("usage_stats_body")
        .max_height(body_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let columns = balance_sections(sections, column_count);
            ui.columns(column_count, |column_uis| {
                for (column_ui, column_sections) in column_uis.iter_mut().zip(columns.iter()) {
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
}

/// Supporting copy for the Usage tab header.
///
/// The two lines say different useful things — where the numbers come from,
/// and what to do when there are none — so instead of picking one, both are
/// returned and the header crossfades between them. With live data recorded
/// the "no data yet" line has nothing to add, so it drops out.
pub fn usage_descriptions(usage_stats: &UsageStore, text: &LocaleText) -> Vec<&'static str> {
    if usage_stats.is_empty() {
        vec![
            text.desktop_settings.usage_no_live_data,
            text.desktop_settings.usage_session_hint,
        ]
    } else {
        vec![text.desktop_settings.usage_session_hint]
    }
}

pub fn usage_dialog_layout(viewport_size: egui::Vec2) -> UsageDialogLayout {
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
    providers: ProviderEnabled,
) -> BTreeMap<String, Vec<&ModelConfig>> {
    let mut sections: BTreeMap<String, Vec<&ModelConfig>> = BTreeMap::new();
    for model in rows {
        if !provider_enabled(&model.provider, providers) {
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

fn provider_enabled(provider: &str, providers: ProviderEnabled) -> bool {
    match provider {
        "groq" => providers.groq,
        "google" | "gemini-live" => providers.gemini,
        "openrouter" => providers.openrouter,
        "nvidia" => providers.nvidia,
        "ollama" => providers.ollama,
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
            1.0_f32,
            blend(theme.card_stroke().color, accent, 0.24),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(crate::gui::theme::space::SNUG))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            render_section_header(ui, text, section_key, accent);

            if section_key == "openrouter" {
                let shared_quota = rows
                    .first()
                    .map(|model| model.localized_quota(lang))
                    .filter(|quota| !quota.is_empty())
                    .unwrap_or("—");
                let summary = usage_stats
                    .get(&UsageKey::provider("openrouter"))
                    .map(|snapshot| snapshot_summary(snapshot, text, theme, now_unix_seconds()));
                let (status, color) = summary
                    .as_ref()
                    .and_then(|summary| summary.headlines.first())
                    .map_or((shared_quota, accent), |line| (&line.label, line.color));
                let response = render_status_strip(
                    ui,
                    text.desktop_settings.usage_shared_quota,
                    status,
                    color,
                    theme,
                    ENDPOINT_ROW_HEIGHT,
                    ENDPOINT_STATUS_FONT_SIZE,
                );
                if let Some(detail) = summary.and_then(|summary| summary.detail) {
                    response.on_hover_text(format!("{shared_quota} · {detail}"));
                }
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
        .inner_margin(egui::Margin::symmetric(
            crate::gui::theme::space::TIGHT,
            crate::gui::theme::space::MICRO,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row_width = ui.available_width();
            let columns = endpoint_columns(row_width);
            let snapshot = usage_stats.get(&UsageKey::endpoint(&model.provider, &model.full_name));
            let status =
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
            crate::gui::model_performance::render_prefix(&mut prefix_ui, model, lang);

            let localized_name = model.localized_name(lang);
            let mut name_ui = cell_ui(
                ui,
                rects.name,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            // No explicit tooltip: egui already shows the full text when a
            // truncated label elides, and adding one stacks two popups.
            name_ui.add(
                egui::Label::new(
                    egui::RichText::new(localized_name)
                        .size(ENDPOINT_NAME_FONT_SIZE)
                        .strong(),
                )
                .truncate(),
            );

            let mut id_ui = cell_ui(
                ui,
                rects.id,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            id_ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("· {}", model.full_name))
                        .monospace()
                        .size(ENDPOINT_ID_FONT_SIZE)
                        .color(theme.on_surface_variant()),
                )
                .truncate(),
            );

            if let Some(headline) = rotating_headline(&status.headlines, ui) {
                // Right-aligned so the quota figures form one column edge and
                // no slack is stranded between them and the card border.
                let mut status_ui = cell_ui(
                    ui,
                    rects.status,
                    egui::Layout::right_to_left(egui::Align::Center),
                );
                let response = status_ui.add(
                    egui::Label::new(
                        egui::RichText::new(&headline.label)
                            .size(ENDPOINT_STATUS_FONT_SIZE)
                            .color(headline.color),
                    )
                    .truncate()
                    .show_tooltip_when_elided(status.detail.is_none()),
                );
                if let Some(detail) = &status.detail {
                    response.on_hover_text(detail);
                }
            }

            row_response.on_hover_text(format!("{} · {}", model.provider, model.full_name));
        });
}

fn rotating_headline<'a>(headlines: &'a [StatusLine], ui: &egui::Ui) -> Option<&'a StatusLine> {
    if headlines.len() < 2 {
        return headlines.first();
    }
    let slot = ui.input(|input| input.time) / METRIC_HOLD_SECONDS;
    ui.ctx().request_repaint_after(Duration::from_secs_f64(
        (1.0 - slot.fract()) * METRIC_HOLD_SECONDS,
    ));
    headlines.get(slot as usize % headlines.len())
}

struct EndpointStatus {
    headlines: Vec<StatusLine>,
    detail: Option<String>,
}

struct StatusLine {
    label: String,
    color: egui::Color32,
}

fn endpoint_status(
    snapshot: Option<&UsageSnapshot>,
    model: &ModelConfig,
    provider_quota_is_shared: bool,
    text: &LocaleText,
    lang: &str,
    theme: &AppTheme,
) -> EndpointStatus {
    if let Some(snapshot) = snapshot {
        return snapshot_summary(snapshot, text, theme, now_unix_seconds());
    }
    let quota = model.localized_quota(lang);
    let headlines = if provider_quota_is_shared || quota.is_empty() {
        Vec::new()
    } else {
        vec![StatusLine {
            label: quota.to_string(),
            color: theme.on_surface_variant(),
        }]
    };
    EndpointStatus {
        headlines,
        detail: None,
    }
}

fn snapshot_summary(
    snapshot: &UsageSnapshot,
    text: &LocaleText,
    theme: &AppTheme,
    now: u64,
) -> EndpointStatus {
    let freshness = freshness_at(snapshot.observed_at_unix_seconds, now);
    let mut headlines = Vec::with_capacity(snapshot.metrics.len());
    let mut segments = Vec::with_capacity(snapshot.metrics.len() + 1);
    for metric in &snapshot.metrics {
        let label = metric_headline(metric, text.desktop_settings.usage_quota_left);
        segments.push(match &metric.reset {
            Some(reset) => format!("{label} ↻{reset}"),
            None => label.clone(),
        });
        headlines.push(StatusLine {
            label,
            color: metric_color(metric, freshness, theme),
        });
    }

    let minutes = now
        .saturating_sub(snapshot.observed_at_unix_seconds)
        .div_ceil(60);
    segments.push(match freshness {
        UsageFreshness::Fresh => text.desktop_settings.usage_updated_now.to_string(),
        UsageFreshness::Aging => format!("{minutes} {}", text.desktop_settings.usage_minutes_ago),
        UsageFreshness::Stale => format!(
            "{} · {minutes} {}",
            text.desktop_settings.usage_stale, text.desktop_settings.usage_minutes_ago
        ),
    });
    EndpointStatus {
        headlines,
        detail: Some(segments.join(" · ")),
    }
}

fn metric_headline(metric: &UsageMetric, quota_left: &str) -> String {
    let ratio = format!(
        "{}/{}",
        metric.remaining.as_deref().unwrap_or("—"),
        metric.limit.as_deref().unwrap_or("—")
    );
    format!(
        "{} {}",
        metric.kind.label(),
        quota_left.replace("{quota}", &ratio)
    )
}

fn metric_color(
    metric: &UsageMetric,
    freshness: UsageFreshness,
    theme: &AppTheme,
) -> egui::Color32 {
    if freshness != UsageFreshness::Fresh {
        return theme.on_surface_variant();
    }
    match remaining_ratio(metric) {
        Some(ratio) if ratio <= LOW_QUOTA_RATIO => theme.warning(),
        Some(_) => theme.success(),
        None => theme.on_surface_variant(),
    }
}

fn remaining_ratio(metric: &UsageMetric) -> Option<f64> {
    let parse = |value: &Option<String>| {
        value
            .as_deref()
            .and_then(|raw| raw.replace(',', "").parse::<f64>().ok())
            .filter(|value| value.is_finite())
    };
    match (parse(&metric.remaining), parse(&metric.limit)) {
        (Some(remaining), Some(limit)) if limit > 0.0 && (0.0..=limit).contains(&remaining) => {
            Some(remaining / limit)
        }
        _ => None,
    }
}

fn provider_name(provider: &str) -> String {
    crate::model_config::provider_full_name(provider).to_string()
}

fn provider_dashboard(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("https://console.groq.com/docs/rate-limits"),
        "google" => Some("https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit"),
        "openrouter" => Some("https://openrouter.ai/activity"),
        _ => None,
    }
}

fn provider_accent(theme: &AppTheme, provider: &str) -> egui::Color32 {
    match provider {
        "groq" => theme.warning(),
        "google" => theme.accent_help(),
        "openrouter" => theme.accent_fill(),
        "taalas" => theme.accent_three_d_generator(),
        _ => theme.on_surface_variant(),
    }
}

#[cfg(test)]
#[path = "usage_stats_tests.rs"]
mod tests;
