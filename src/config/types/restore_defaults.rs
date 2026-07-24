use serde::{Deserialize, Serialize};

/// Categories offered by the selective "Restore defaults" dialog.
///
/// This is persisted so reopening the dialog (including after an app restart)
/// restores the user's last checklist instead of silently selecting a different
/// scope. New installs and configs created before this field existed start with
/// every category selected; protected user data and hotkeys remain untouched.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct RestoreDefaultsSelection {
    pub presets: bool,
    pub app_settings: bool,
    pub model_settings: bool,
    pub audio_settings: bool,
    pub shortcuts_and_mini_apps: bool,
    pub local_data: bool,
}

impl RestoreDefaultsSelection {
    pub fn any(self) -> bool {
        self.presets
            || self.app_settings
            || self.model_settings
            || self.audio_settings
            || self.shortcuts_and_mini_apps
            || self.local_data
    }

    pub fn all(self) -> bool {
        self.presets
            && self.app_settings
            && self.model_settings
            && self.audio_settings
            && self.shortcuts_and_mini_apps
            && self.local_data
    }

    pub fn set_all(&mut self, selected: bool) {
        self.presets = selected;
        self.app_settings = selected;
        self.model_settings = selected;
        self.audio_settings = selected;
        self.shortcuts_and_mini_apps = selected;
        self.local_data = selected;
    }
}

impl Default for RestoreDefaultsSelection {
    fn default() -> Self {
        Self {
            presets: true,
            app_settings: true,
            model_settings: true,
            audio_settings: true,
            shortcuts_and_mini_apps: true,
            local_data: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RestoreDefaultsSelection;

    #[test]
    fn defaults_to_every_category_selected() {
        let selection = RestoreDefaultsSelection::default();
        assert!(selection.all());
        assert!(selection.any());
    }

    #[test]
    fn bulk_selection_updates_every_category() {
        let mut selection = RestoreDefaultsSelection::default();
        selection.set_all(false);
        assert!(!selection.any());

        selection.set_all(true);
        assert!(selection.all());
    }
}
