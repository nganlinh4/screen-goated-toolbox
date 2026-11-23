use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, format_size, get_dir_size, invalidate_probe_cache, invalidate_size_cache,
    start_install, start_removal,
};

/// Declarative description of a single downloadable model row (title, probe key,
/// download/remove hooks). The ~50-line row body is rendered once by
/// [`render_model_row`] so it is not duplicated per model.
pub(super) struct ModelRowSpec {
    pub(super) model_probe: &'static str,
    pub(super) model_title: &'static str,
    pub(super) progress_key: &'static str,
    pub(super) model_notice: fn() -> Option<String>,
    pub(super) is_model_downloaded: fn() -> bool,
    pub(super) model_dir: fn() -> PathBuf,
    pub(super) download_model: fn(Arc<AtomicBool>, bool) -> anyhow::Result<()>,
    pub(super) remove_model: fn() -> anyhow::Result<()>,
    /// `None` means the row has an intrinsic delivery path. A predicate is
    /// supplied for build-generated packages that may be intentionally absent.
    pub(super) is_available: Option<fn() -> bool>,
    /// Optional description rendered after the row (the per-model section cards
    /// render the description below the row; the TTS cards render it above and
    /// pass `None` here).
    pub(super) description: Option<&'static str>,
    /// Whether to add a 4px gap before the notice line (matches the per-model
    /// section cards which space the notice; the TTS cards do not).
    pub(super) space_before_notice: bool,
}

pub(super) fn render_model_row(ui: &mut egui::Ui, text: &LocaleText, spec: &ModelRowSpec) {
    let theme = AppTheme::from_ui(ui);
    let managed = &text.auxiliary.managed_tools;
    let notice = (spec.model_notice)();
    let state = if cached_probe(spec.model_probe, spec.is_model_downloaded) {
        let size = get_dir_size(&(spec.model_dir)());
        ComponentState::Installed {
            summary: managed
                .tool_status_installed
                .replace("{}", &format_size(size)),
        }
    } else if spec.is_available.is_some_and(|available| !available()) {
        ComponentState::Unavailable
    } else {
        ComponentState::Missing
    };
    let operations = RowOperations {
        install: Some(spec.progress_key),
        removal: spec.model_probe,
    };
    match render_component_row(ui, text, spec.model_title, operations, state) {
        Some(RowAction::Install) => start_model_install(spec),
        Some(RowAction::Remove) => start_model_removal(spec),
        None => {}
    }
    if let Some(description) = spec.description {
        ui.label(description);
    }
    if let Some(message) = notice {
        if spec.space_before_notice {
            ui.add_space(4.0);
        }
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

fn start_model_install(spec: &ModelRowSpec) {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let download_model = spec.download_model;
    let probe = spec.model_probe;
    let model_dir = spec.model_dir;
    start_install(spec.progress_key, move || {
        let _ = download_model(stop_signal, true);
        invalidate_probe_cache(probe);
        invalidate_size_cache(&model_dir());
    });
}

fn start_model_removal(spec: &ModelRowSpec) {
    let probe = spec.model_probe;
    let title = spec.model_title.to_string();
    let model_dir = spec.model_dir;
    let remove_model = spec.remove_model;
    start_removal(probe, title, remove_model, move || {
        invalidate_size_cache(&model_dir());
        invalidate_probe_cache(probe);
    });
}
