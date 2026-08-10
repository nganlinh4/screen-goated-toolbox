//! Downloaded Tools card for independently delivered mini-app interfaces.

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::model_card::{ModelRowSpec, render_model_row};
use super::utils::tool_card;

const PROBE_PROMPT_DJ_WEB: &str = "downloaded-tools:prompt-dj-web";
const PROBE_TTS_PLAYGROUND_WEB: &str = "downloaded-tools:tts-playground-web";

fn no_notice() -> Option<String> {
    None
}

pub(super) fn render_web_apps_card(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_web_apps_card);
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_PROMPT_DJ_WEB,
                model_title: managed.tool_prompt_dj_interface,
                model_download_title: crate::overlay::prompt_dj::web_asset_download_title(),
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::prompt_dj::are_web_assets_installed,
                model_dir: crate::overlay::prompt_dj::web_assets_dir,
                download_model: crate::overlay::prompt_dj::download_web_assets,
                remove_model: crate::overlay::prompt_dj::remove_web_assets,
                description: Some(managed.tool_desc_first_use_interface),
                space_before_notice: true,
            },
        );
        ui.add_space(8.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_TTS_PLAYGROUND_WEB,
                model_title: managed.tool_tts_playground_interface,
                model_download_title: crate::overlay::tts_playground::web_asset_download_title(),
                model_notice: no_notice,
                is_model_downloaded: crate::overlay::tts_playground::are_web_assets_installed,
                model_dir: crate::overlay::tts_playground::web_assets_dir,
                download_model: crate::overlay::tts_playground::download_web_assets,
                remove_model: crate::overlay::tts_playground::remove_web_assets,
                description: Some(managed.tool_desc_tts_playground_interface),
                space_before_notice: true,
            },
        );
    });
}
