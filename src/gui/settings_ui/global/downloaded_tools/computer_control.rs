//! Downloaded Tools card for the Computer Control agent's local UI-element
//! detector (UI-DETR-1 ONNX). Shows download/installed/missing state and live
//! download progress (the same `REALTIME_STATE` channel the auto-copy badge uses),
//! and lets the user pre-download or delete it. The model also auto-downloads on
//! first use; both routes share `download_detector_model`, so progress shows here.

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::model_card::{ModelRowSpec, render_model_row};
use super::utils::tool_card;

const PROBE_UI_DETECTOR: &str = "downloaded-tools:ui-detector";

fn no_notice() -> Option<String> {
    None
}

pub(super) fn render_computer_control_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading("Computer Control");
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_UI_DETECTOR,
                model_title: "UI element detector",
                model_download_title: crate::overlay::computer_control::DETECTOR_DOWNLOAD_TITLE,
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::computer_control::is_detector_downloaded,
                model_dir: crate::overlay::computer_control::detector_model_dir,
                download_model: crate::overlay::computer_control::download_detector_model,
                remove_model: crate::overlay::computer_control::remove_detector_model,
                description: Some(
                    "Local ONNX detector (UI-DETR-1, MIT) that finds clickable regions on \
canvas / game UIs the accessibility tree can't see, for the Computer Control agent. \
Auto-downloads on first use.",
                ),
                space_before_notice: true,
            },
        );
    });
}
