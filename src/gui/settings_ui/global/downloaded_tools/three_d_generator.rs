//! Downloaded Tools card for creation mini-app components.

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::model_card::{ModelRowSpec, render_model_row};
use super::utils::tool_card;

const PROBE_3D_GENERATOR_RUNTIME: &str = "downloaded-tools:3d-generator-runtime";
const PROBE_3D_GENERATOR_WEB: &str = "downloaded-tools:3d-generator-web";

fn no_notice() -> Option<String> {
    None
}

pub(super) fn render_three_d_generator_card(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_creation_card);
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_3D_GENERATOR_WEB,
                model_title: managed.tool_creation_interface,
                model_download_title: crate::overlay::three_d_generator::web_asset_download_title(),
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::three_d_generator::are_web_assets_installed,
                model_dir: crate::overlay::three_d_generator::web_assets_dir,
                download_model: crate::overlay::three_d_generator::download_web_assets,
                remove_model: crate::overlay::three_d_generator::remove_web_assets,
                description: Some(managed.tool_desc_first_use_interface),
                space_before_notice: true,
            },
        );
        ui.add_space(8.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_3D_GENERATOR_RUNTIME,
                model_title: managed.tool_creation_engine,
                model_download_title: crate::overlay::three_d_generator::runtime_download_title(),
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::three_d_generator::is_runtime_installed,
                model_dir: crate::overlay::three_d_generator::runtime_bundle_dir,
                download_model: crate::overlay::three_d_generator::download_runtime,
                remove_model: crate::overlay::three_d_generator::remove_runtime,
                description: Some(managed.tool_desc_creation_engine),
                space_before_notice: true,
            },
        );
    });
}
