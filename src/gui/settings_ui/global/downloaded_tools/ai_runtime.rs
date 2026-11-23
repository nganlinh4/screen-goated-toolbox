use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::status_cache::StatusCache;
use super::utils::{format_size, start_install, start_removal, tool_card};

static ONNX_STATUS: LazyLock<
    Arc<StatusCache<crate::component_registry::local_asr::ComponentStatus>>,
> = LazyLock::new(StatusCache::new);
static VC_STATUS: LazyLock<
    Arc<StatusCache<crate::component_registry::vc_runtime::VcRuntimeStatus>>,
> = LazyLock::new(StatusCache::new);
const STATUS_TTL: Duration = Duration::from_millis(500);
const INSTALL_ONNX: &str = "downloaded-tools:install-onnx-runtime";
const INSTALL_VC: &str = "downloaded-tools:install-vc-runtime";

pub(super) fn invalidate_statuses() {
    ONNX_STATUS.invalidate();
    VC_STATUS.invalidate();
}

fn start_onnx_install() {
    ONNX_STATUS.invalidate();
    start_install(INSTALL_ONNX, || {
        let _ = crate::component_registry::local_asr::start_install();
        ONNX_STATUS.invalidate();
    });
}

fn start_vc_install() {
    VC_STATUS.invalidate();
    start_install(INSTALL_VC, || {
        let _ = crate::component_registry::vc_runtime::start_install();
        VC_STATUS.invalidate();
    });
}

const REMOVE_ONNX_RUNTIME: &str = "remove:onnx-directml-runtime";
const REMOVE_VC_RUNTIME: &str = "remove:vc14-x64-runtime";

/// Native runtimes that downloadable engines share: ONNX/DirectML and the VC++
/// redistributable.
pub(super) fn render_shared_runtimes_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_shared_runtimes_card);
        ui.add_space(4.0);
        render_onnx_runtime_content(ui, text);
        ui.add_space(4.0);
        render_vc_runtime_content(ui, text);
    });
}

fn render_onnx_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::component_registry::local_asr::{self, ComponentStatus};

    let theme = AppTheme::from_ui(ui);
    let wake = ui.ctx().clone();
    let status = ONNX_STATUS.get(STATUS_TTL, local_asr::current_status, move || {
        wake.request_repaint()
    });
    ui.ctx().request_repaint_after(STATUS_TTL);
    let state = match &status {
        None => ComponentState::Checking,
        Some(ComponentStatus::Installed { bytes, version }) => ComponentState::Installed {
            summary: installed_summary(text, *bytes, version),
        },
        Some(ComponentStatus::Installing { progress }) => {
            ComponentState::Installing { percent: *progress }
        }
        Some(ComponentStatus::Error(message)) => ComponentState::Failed { message },
        Some(ComponentStatus::Missing) => ComponentState::Missing,
        Some(ComponentStatus::Unavailable) => ComponentState::Unavailable,
    };
    let label = text.auxiliary.managed_tools.tool_onnx_runtime;
    let operations = RowOperations {
        install: Some(INSTALL_ONNX),
        removal: REMOVE_ONNX_RUNTIME,
    };
    match render_component_row(ui, text, label, operations, state) {
        Some(RowAction::Install) => start_onnx_install(),
        Some(RowAction::Remove) => start_removal(
            REMOVE_ONNX_RUNTIME,
            label.to_string(),
            local_asr::remove,
            || ONNX_STATUS.invalidate(),
        ),
        None => {}
    }
    ui.label(text.auxiliary.managed_tools.tool_desc_onnx_runtime);
    ui.label(
        egui::RichText::new(version_or_unavailable(local_asr::version_label(), text))
            .small()
            .color(egui::Color32::GRAY),
    );
    if let Some(message) = local_asr::current_notice() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

fn render_vc_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::component_registry::vc_runtime::{self, VcRuntimeStatus};

    let theme = AppTheme::from_ui(ui);
    let wake = ui.ctx().clone();
    let status = VC_STATUS.get(STATUS_TTL, vc_runtime::current_status, move || {
        wake.request_repaint()
    });
    ui.ctx().request_repaint_after(STATUS_TTL);
    let state = match &status {
        None => ComponentState::Checking,
        Some(VcRuntimeStatus::Installed { bytes, version }) => ComponentState::Installed {
            summary: installed_summary(text, *bytes, version),
        },
        Some(VcRuntimeStatus::Installing { progress }) => {
            ComponentState::Installing { percent: *progress }
        }
        Some(VcRuntimeStatus::Error(message)) => ComponentState::Failed { message },
        Some(VcRuntimeStatus::Missing) => ComponentState::Missing,
        Some(VcRuntimeStatus::Unavailable) => ComponentState::Unavailable,
    };
    let title = text.auxiliary.managed_tools.tool_vc_runtime;
    let operations = RowOperations {
        install: Some(INSTALL_VC),
        removal: REMOVE_VC_RUNTIME,
    };
    match render_component_row(ui, text, title, operations, state) {
        Some(RowAction::Install) => start_vc_install(),
        Some(RowAction::Remove) => start_removal(
            REMOVE_VC_RUNTIME,
            title.to_string(),
            vc_runtime::remove,
            || VC_STATUS.invalidate(),
        ),
        None => {}
    }
    ui.label(text.auxiliary.managed_tools.tool_desc_vc_runtime);
    ui.label(
        egui::RichText::new(version_or_unavailable(vc_runtime::version_label(), text))
            .small()
            .color(egui::Color32::GRAY),
    );
    if let Some(message) = vc_runtime::current_notice() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

fn installed_summary(text: &LocaleText, bytes: u64, version: &str) -> String {
    format!(
        "{} · {version}",
        text.auxiliary
            .managed_tools
            .tool_status_installed
            .replace("{}", &format_size(bytes))
    )
}

fn version_or_unavailable(version: Option<String>, text: &LocaleText) -> String {
    version.unwrap_or_else(|| {
        format!(
            "{} (x64)",
            text.auxiliary.managed_tools.tool_status_unavailable
        )
    })
}
