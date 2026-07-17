use crate::gui::locale::AuxiliaryLocaleText;

pub(super) fn get() -> AuxiliaryLocaleText {
    AuxiliaryLocaleText {
        download: super::download::get(),
        managed_tools: super::managed_tools::get(),
        continuous_mode_activated: "Preset \"{preset}\" will run continuously. Press ESC or {hotkey} to exit",
        win_select_title: "Select a Window to Record",
        win_select_subtitle: "Press Escape or click outside to cancel",
        win_select_count: "{} windows",
        win_select_display_only_badge: "DISPLAY ONLY",
        win_select_display_only_title: "Use Display Capture",
        win_select_display_only_message: "This fullscreen or presentation window cannot be recorded reliably as an individual window. Choose Display capture instead.",
    }
}
