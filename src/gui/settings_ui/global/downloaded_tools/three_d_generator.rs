//! Downloaded Tools card for creation mini-app components.

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::model_card::{ModelRowSpec, render_model_row};
use super::utils::tool_card;

const PROBE_DEPTH_ANYTHING_3: &str = "downloaded-tools:depth-anything-3";
const PROBE_3D_GENERATOR_RUNTIME: &str = "downloaded-tools:3d-generator-runtime";

fn no_notice() -> Option<String> {
    None
}

pub(super) fn render_three_d_generator_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading("Creation tools");
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_3D_GENERATOR_RUNTIME,
                model_title: "Native creation engine",
                model_download_title: crate::overlay::three_d_generator::RUNTIME_DOWNLOAD_TITLE,
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::three_d_generator::is_runtime_installed,
                model_dir: crate::overlay::three_d_generator::runtime_bundle_dir,
                download_model: crate::overlay::three_d_generator::download_runtime,
                remove_model: crate::overlay::three_d_generator::remove_runtime,
                description: Some(
                    "Shared native worker for 3D models and SVG vectors. Auto-downloads on first use.",
                ),
                space_before_notice: true,
            },
        );
        ui.add_space(10.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_DEPTH_ANYTHING_3,
                model_title: "Depth Anything 3 Small",
                model_download_title: crate::overlay::three_d_generator::DEPTH_DOWNLOAD_TITLE,
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::three_d_generator::is_depth_model_downloaded,
                model_dir: crate::overlay::three_d_generator::depth_model_dir,
                download_model: crate::overlay::three_d_generator::download_depth_model,
                remove_model: crate::overlay::three_d_generator::remove_depth_model,
                description: Some(
                    "Local ONNX depth model used to turn source images into the live 3D creation preview. Auto-downloads on first use.",
                ),
                space_before_notice: true,
            },
        );
    });
}
