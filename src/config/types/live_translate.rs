use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveTranslateInterface {
    #[default]
    Standard,
    Minimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LiveTranslateSettings {
    pub interface: LiveTranslateInterface,
    pub hotkeys: Vec<Hotkey>,
}

impl LiveTranslateSettings {
    pub fn restore_defaults_preserving_hotkeys(&mut self) {
        let hotkeys = std::mem::take(&mut self.hotkeys);
        *self = Self {
            hotkeys,
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{LiveTranslateInterface, LiveTranslateSettings};

    #[test]
    fn restoring_defaults_keeps_global_hotkeys() {
        let hotkeys = vec![crate::config::Hotkey::new(9, "Live", 10)];
        let mut settings = LiveTranslateSettings {
            interface: LiveTranslateInterface::Minimal,
            hotkeys: hotkeys.clone(),
        };

        settings.restore_defaults_preserving_hotkeys();

        assert_eq!(settings.hotkeys, hotkeys);
        assert_eq!(settings.interface, LiveTranslateInterface::Standard);
    }
}
