use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LiveTranslateSettings {
    pub hotkeys: Vec<Hotkey>,
}

#[cfg(test)]
mod tests {
    use super::LiveTranslateSettings;

    #[test]
    fn removed_interface_is_accepted_but_not_persisted() {
        let settings: LiveTranslateSettings = serde_json::from_value(serde_json::json!({
            "interface": "minimal",
            "hotkeys": []
        }))
        .unwrap();
        let serialized = serde_json::to_value(settings).unwrap();

        assert!(serialized.get("interface").is_none());
    }
}
