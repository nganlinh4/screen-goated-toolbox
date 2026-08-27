use super::Config;

impl Config {
    /// Pulls only persistent controls that a running Live Translate overlay can edit.
    /// Preflight-only settings and unrelated config remain owned by this instance.
    pub(crate) fn sync_live_translate_overlay_controls_from(&mut self, source: &Self) -> bool {
        let changed = self.realtime_audio_source != source.realtime_audio_source
            || self.realtime_target_language != source.realtime_target_language
            || self.realtime_translation_model != source.realtime_translation_model
            || self.realtime_transcription_model != source.realtime_transcription_model
            || self.realtime_custom_vocabulary != source.realtime_custom_vocabulary
            || self.realtime_transcription_language != source.realtime_transcription_language
            || self.realtime_font_size != source.realtime_font_size;

        self.realtime_audio_source
            .clone_from(&source.realtime_audio_source);
        self.realtime_target_language
            .clone_from(&source.realtime_target_language);
        self.realtime_translation_model
            .clone_from(&source.realtime_translation_model);
        self.realtime_transcription_model
            .clone_from(&source.realtime_transcription_model);
        self.realtime_custom_vocabulary
            .clone_from(&source.realtime_custom_vocabulary);
        self.realtime_transcription_language
            .clone_from(&source.realtime_transcription_language);
        self.realtime_font_size = source.realtime_font_size;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::config::Hotkey;

    #[test]
    fn overlay_controls_sync_without_clobbering_preflight_or_unrelated_settings() {
        let mut modal = Config::default();
        modal.live_translate.hotkeys = vec![Hotkey::new(7, "Live Translate", 3)];
        modal.ui_language = "ko".to_string();

        let mut overlay = modal.clone();
        overlay.realtime_audio_source = "mic".to_string();
        overlay.realtime_target_language = "Japanese".to_string();
        overlay.realtime_translation_model = "google-gtx".to_string();
        overlay.realtime_transcription_model = "zipformer".to_string();
        overlay.realtime_custom_vocabulary = vec!["Codex".to_string(), "WebView2".to_string()];
        overlay.realtime_transcription_language = "ja".to_string();
        overlay.realtime_font_size = 32;
        overlay.live_translate.hotkeys.clear();
        overlay.ui_language = "vi".to_string();

        assert!(modal.sync_live_translate_overlay_controls_from(&overlay));
        assert_eq!(modal.realtime_audio_source, "mic");
        assert_eq!(modal.realtime_target_language, "Japanese");
        assert_eq!(modal.realtime_translation_model, "google-gtx");
        assert_eq!(modal.realtime_transcription_model, "zipformer");
        assert_eq!(modal.realtime_custom_vocabulary, ["Codex", "WebView2"]);
        assert_eq!(modal.realtime_transcription_language, "ja");
        assert_eq!(modal.realtime_font_size, 32);
        assert_eq!(modal.live_translate.hotkeys.len(), 1);
        assert_eq!(modal.ui_language, "ko");
        assert!(!modal.sync_live_translate_overlay_controls_from(&overlay));
    }

    #[test]
    fn custom_vocabulary_survives_config_round_trip() {
        let config = Config {
            realtime_custom_vocabulary: vec!["WebView2".to_string(), "SGT".to_string()],
            ..Default::default()
        };

        let saved = serde_json::to_string(&config).unwrap();
        let restored: Config = serde_json::from_str(&saved).unwrap();

        assert_eq!(restored.realtime_custom_vocabulary, ["WebView2", "SGT"]);
    }
}
