use crate::gui::locale::AuxiliaryLocaleText;

pub(super) fn get() -> AuxiliaryLocaleText {
    AuxiliaryLocaleText {
        download: super::download::get(),
        managed_tools: super::managed_tools::get(),
        continuous_mode_activated: "Preset \"{preset}\" will run continuously. Press ESC or {hotkey} to exit",
        win_select_title: "Select a Window to Record",
        win_select_subtitle: "Press Escape or click outside to cancel",
        win_select_count: "{} windows",
    }
}
