use super::{DownloadLocaleText, ManagedToolsLocaleText};

pub struct AuxiliaryLocaleText {
    pub download: DownloadLocaleText,
    pub managed_tools: ManagedToolsLocaleText,
    pub continuous_mode_activated: &'static str,
    pub win_select_title: &'static str,
    pub win_select_subtitle: &'static str,
    pub win_select_count: &'static str,
}
