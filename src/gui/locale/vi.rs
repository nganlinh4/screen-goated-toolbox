use super::LocaleText;

mod auxiliary;
mod desktop_settings;
mod download;
mod global_settings;
mod managed_tools;
mod model_catalog;
mod overlay;
mod preset_basics;
mod preset_editor;
mod realtime;
mod shell;
mod tool_runtime;
mod translation_gummy;
mod tts_advanced;
mod tts_playground;
mod tts_settings;
mod workspace;

pub fn get() -> LocaleText {
    LocaleText {
        locale_code: "vi",
        workspace: workspace::get(),
        preset_basics: preset_basics::get(),
        desktop_settings: desktop_settings::get(),
        preset_editor: preset_editor::get(),
        global_settings: global_settings::get(),
        tts_playground: tts_playground::get(),
        model_catalog: model_catalog::get(),
        tts_settings: tts_settings::get(),
        tts_advanced: tts_advanced::get(),
        realtime: realtime::get(),
        shell: shell::get(),
        translation_gummy: translation_gummy::get(),
        tool_runtime: tool_runtime::get(),
        overlay: overlay::get(),
        auxiliary: auxiliary::get(),
    }
}
