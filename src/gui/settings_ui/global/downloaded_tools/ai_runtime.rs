use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::utils::format_size;

pub(super) fn render_local_asr_worker_content(ui: &mut egui::Ui, text: &LocaleText) {
    render_local_asr_component(
        ui,
        text,
        crate::component_registry::local_asr::ComponentKind::Worker,
        text.auxiliary.managed_tools.tool_local_asr_worker,
        text.auxiliary.managed_tools.tool_desc_local_asr_worker,
    );
}

pub(super) fn render_onnx_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    render_local_asr_component(
        ui,
        text,
        crate::component_registry::local_asr::ComponentKind::Runtime,
        text.auxiliary.managed_tools.tool_onnx_runtime,
        text.auxiliary.managed_tools.tool_desc_onnx_runtime,
    );
}

fn render_local_asr_component(
    ui: &mut egui::Ui,
    text: &LocaleText,
    kind: crate::component_registry::local_asr::ComponentKind,
    label: &str,
    description: &str,
) {
    use crate::component_registry::local_asr::{self, ComponentStatus};

    let theme = AppTheme::from_ui(ui);
    let status = local_asr::current_status(kind);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match &status {
                #[cfg(debug_assertions)]
                ComponentStatus::Development { bytes } => {
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary
                                .managed_tools
                                .tool_status_development_fmt
                                .replace("{}", &format_size(*bytes)),
                        )
                        .color(theme.success()),
                    );
                }
                ComponentStatus::Installed { bytes, version } => {
                    if ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                        && let Err(error) = local_asr::remove(kind)
                    {
                        crate::log_info!("[Downloaded Tools] component removal failed: {error}");
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {version}",
                            text.auxiliary
                                .managed_tools
                                .tool_status_installed
                                .replace("{}", &format_size(*bytes))
                        ))
                        .color(theme.success()),
                    );
                }
                ComponentStatus::Installing { progress } => {
                    ui.label(format!("{progress:.0}%"));
                    ui.spinner();
                }
                ComponentStatus::Error(message) => {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        let _ = local_asr::start_install(kind);
                    }
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary.managed_tools.tool_status_install_failed,
                        )
                        .color(theme.danger_text()),
                    )
                    .on_hover_text(message);
                }
                ComponentStatus::Missing => {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        let _ = local_asr::start_install(kind);
                    }
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
                ComponentStatus::Unavailable => {
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_unavailable)
                            .color(theme.danger_text()),
                    );
                }
            },
        );
    });
    ui.label(description);
    ui.label(
        egui::RichText::new(version_or_unavailable(local_asr::version_label(kind), text))
            .small()
            .color(egui::Color32::GRAY),
    );
    if let Some(message) = local_asr::current_notice(kind) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

pub(super) fn render_vc_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::component_registry::vc_runtime::{self, VcRuntimeStatus};

    let theme = AppTheme::from_ui(ui);
    let status = vc_runtime::current_status();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.auxiliary.managed_tools.tool_vc_runtime).strong());
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match &status {
                #[cfg(debug_assertions)]
                VcRuntimeStatus::Development { bytes } => {
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary
                                .managed_tools
                                .tool_status_development_fmt
                                .replace("{}", &format_size(*bytes)),
                        )
                        .color(theme.success()),
                    );
                }
                VcRuntimeStatus::Installed { bytes, version } => {
                    if ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        let _ = vc_runtime::remove();
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {version}",
                            text.auxiliary
                                .managed_tools
                                .tool_status_installed
                                .replace("{}", &format_size(*bytes))
                        ))
                        .color(theme.success()),
                    );
                }
                VcRuntimeStatus::Installing { progress } => {
                    ui.label(format!("{progress:.0}%"));
                    ui.spinner();
                }
                VcRuntimeStatus::Error(message) => {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        let _ = vc_runtime::start_install();
                    }
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary.managed_tools.tool_status_install_failed,
                        )
                        .color(theme.danger_text()),
                    )
                    .on_hover_text(message);
                }
                VcRuntimeStatus::Missing => {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        let _ = vc_runtime::start_install();
                    }
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
                VcRuntimeStatus::Unavailable => {
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_unavailable)
                            .color(theme.danger_text()),
                    );
                }
            },
        );
    });
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

fn version_or_unavailable(version: Option<String>, text: &LocaleText) -> String {
    version.unwrap_or_else(|| {
        format!(
            "{} (x64)",
            text.auxiliary.managed_tools.tool_status_unavailable
        )
    })
}
