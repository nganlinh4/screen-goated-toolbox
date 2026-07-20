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
mod text;
mod tips;
mod tool_runtime;
mod translation_gummy;
mod tts_advanced;
mod tts_playground;
mod tts_settings;
mod workspace;

pub use auxiliary::AuxiliaryLocaleText;
pub use desktop_settings::DesktopSettingsLocaleText;
pub use download::DownloadLocaleText;
pub use global_settings::GlobalSettingsLocaleText;
pub use managed_tools::ManagedToolsLocaleText;
pub use model_catalog::ModelCatalogLocaleText;
pub use overlay::OverlayLocaleText;
pub use preset_basics::PresetBasicsLocaleText;
pub use preset_editor::PresetEditorLocaleText;
pub use realtime::RealtimeLocaleText;
pub use shell::ShellLocaleText;
pub use text::LocaleText;
pub use tool_runtime::ToolRuntimeLocaleText;
pub use translation_gummy::TranslationGummyLocaleText;
pub use tts_advanced::TtsAdvancedLocaleText;
pub use tts_playground::TtsPlaygroundLocaleText;
pub use tts_settings::TtsSettingsLocaleText;
pub use workspace::WorkspaceLocaleText;

mod en;
mod ko;
mod vi;

#[cfg(test)]
mod tests;

impl LocaleText {
    pub fn get(lang_code: &str) -> Self {
        match lang_code {
            "vi" => vi::get(),
            "ko" => ko::get(),
            _ => en::get(),
        }
    }

    pub fn hotkey_conflict_message(&self, conflict: &crate::config::HotkeyConflict) -> String {
        use crate::config::{GlobalHotkeyOwner, HotkeyConflict};

        match conflict {
            HotkeyConflict::Global { owner, hotkey_name } => {
                let owner_name = match owner {
                    GlobalHotkeyOwner::ScreenRecord => self.tool_runtime.screen_record_btn,
                    GlobalHotkeyOwner::TranslationGummy => {
                        self.translation_gummy.translation_gummy_title
                    }
                    GlobalHotkeyOwner::ComputerControl => self.shell.computer_control_title,
                };
                self.preset_basics
                    .hotkey_conflict_global_fmt
                    .replace("{hotkey}", hotkey_name)
                    .replace("{owner}", owner_name)
            }
            HotkeyConflict::Preset {
                hotkey_name,
                preset_name,
            } => self
                .preset_basics
                .hotkey_conflict_preset_fmt
                .replace("{hotkey}", hotkey_name)
                .replace("{preset}", preset_name),
        }
    }
}
