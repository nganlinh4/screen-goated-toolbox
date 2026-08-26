//! Size-conscious decoding for the flat, stable config file schema.
//!
//! Serde's derived visitor for this wide struct emitted one very large dispatch
//! function. Decode through a JSON object instead: defaults keep old files
//! forward-compatible, unknown future fields remain harmless, and malformed
//! values still fail the whole load so the caller can preserve the file.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::Config;

fn replace_field<T, E>(object: &mut Map<String, Value>, name: &str, target: &mut T) -> Result<(), E>
where
    T: DeserializeOwned,
    E: serde::de::Error,
{
    let Some(value) = object.remove(name) else {
        return Ok(());
    };
    *target = serde_json::from_value(value)
        .map_err(|error| E::custom(format!("invalid config field `{name}`: {error}")))?;
    Ok(())
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut object = Map::<String, Value>::deserialize(deserializer)?;
        let mut config = Self::default();

        macro_rules! field {
            ($name:ident) => {
                replace_field::<_, D::Error>(&mut object, stringify!($name), &mut config.$name)?;
            };
        }

        field!(api_key);
        field!(gemini_api_key);
        field!(openrouter_api_key);
        field!(nvidia_api_key);
        field!(presets);
        field!(active_preset_idx);
        field!(preset_profiles);
        field!(active_preset_profile_idx);
        field!(theme_mode);
        field!(ui_language);
        field!(max_history_items);
        field!(max_screen_record_projects);
        field!(max_screen_record_recent_uploads);
        field!(cc_max_memory_items);
        field!(favorite_overlay_opacity);
        field!(result_controls_discovery_pulse_count);
        field!(restore_defaults_selection);
        field!(pending_preset_model_update);
        field!(start_in_tray);
        field!(show_startup_animation);
        field!(run_as_admin_on_startup);
        field!(run_at_startup);
        field!(authorized_startup_path);
        field!(use_groq);
        field!(use_gemini);
        field!(use_openrouter);
        field!(use_nvidia);
        field!(use_ollama);
        field!(model_priority_chains);
        field!(adaptive_model_priority);
        field!(ollama_base_url);
        field!(ollama_vision_model);
        field!(ollama_text_model);
        field!(custom_models);
        field!(realtime_translation_model);
        field!(realtime_transcription_model);
        field!(realtime_transcription_language);
        field!(realtime_font_size);
        field!(realtime_transcription_size);
        field!(realtime_translation_size);
        field!(realtime_audio_source);
        field!(realtime_target_language);
        field!(tts_method);
        field!(tts_voice);
        field!(tts_speed);
        field!(tts_gemini_live_model);
        field!(tts_output_device);
        field!(tts_language_conditions);
        field!(edge_tts_settings);
        field!(step_audio_settings);
        field!(step_audio_reference_voices);
        field!(magpie_settings);
        field!(kokoro_settings);
        field!(supertonic_settings);
        field!(vieneu_settings);
        field!(voxtral_settings);
        field!(tts_playground);
        field!(show_favorite_bubble);
        field!(favorite_bubble_position);
        field!(favorites_keep_open);
        field!(favorite_bubble_size);
        field!(clear_webview_on_startup);
        field!(screen_record_hotkeys);
        field!(computer_control_hotkeys);
        field!(screen_record_window_size);
        field!(translation_gummy);
        field!(screen_translate);
        field!(live_translate);

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_config_round_trips_through_sectioned_decoder() {
        let config = Config::default();
        let encoded = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.ui_language, config.ui_language);
        assert_eq!(decoded.presets.len(), config.presets.len());
        assert_eq!(
            decoded.favorite_overlay_opacity,
            config.favorite_overlay_opacity
        );
        assert_eq!(decoded.screen_translate, config.screen_translate);
    }

    #[test]
    fn absent_and_unknown_fields_preserve_defaults() {
        let decoded: Config =
            serde_json::from_str(r#"{"future_field":true,"graphics_mode":"minimal"}"#).unwrap();
        let defaults = Config::default();

        assert_eq!(decoded.ui_language, defaults.ui_language);
        assert_eq!(decoded.presets.len(), defaults.presets.len());
        assert!(
            !serde_json::to_value(decoded)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("graphics_mode")
        );
    }

    #[test]
    fn malformed_known_field_is_not_silently_ignored() {
        let error =
            serde_json::from_str::<Config>(r#"{"favorite_overlay_opacity":"opaque"}"#).unwrap_err();
        assert!(error.to_string().contains("favorite_overlay_opacity"));
    }
}
