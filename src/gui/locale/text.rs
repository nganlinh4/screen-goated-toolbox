use super::{
    AuxiliaryLocaleText, DesktopSettingsLocaleText, GlobalSettingsLocaleText,
    ModelCatalogLocaleText, OverlayLocaleText, PresetBasicsLocaleText, PresetEditorLocaleText,
    RealtimeLocaleText, ShellLocaleText, ToolRuntimeLocaleText, TranslationGummyLocaleText,
    TtsAdvancedLocaleText, TtsPlaygroundLocaleText, TtsSettingsLocaleText, WorkspaceLocaleText,
};

pub struct LocaleText {
    pub locale_code: &'static str,
    pub workspace: WorkspaceLocaleText,
    pub preset_basics: PresetBasicsLocaleText,
    pub desktop_settings: DesktopSettingsLocaleText,
    pub preset_editor: PresetEditorLocaleText,
    pub global_settings: GlobalSettingsLocaleText,
    pub tts_playground: TtsPlaygroundLocaleText,
    pub model_catalog: ModelCatalogLocaleText,
    pub tts_settings: TtsSettingsLocaleText,
    pub tts_advanced: TtsAdvancedLocaleText,
    pub realtime: RealtimeLocaleText,
    pub shell: ShellLocaleText,
    pub translation_gummy: TranslationGummyLocaleText,
    pub tool_runtime: ToolRuntimeLocaleText,
    pub overlay: OverlayLocaleText,
    pub auxiliary: AuxiliaryLocaleText,
}
