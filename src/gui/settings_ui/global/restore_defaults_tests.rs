use super::{apply_selected_config_defaults_from, render_restore_defaults_modal};
use crate::config::{Config, Hotkey, RestoreDefaultsSelection, ThemeMode};
use crate::gui::locale::LocaleText;
use auto_launch::AutoLaunch;
use eframe::egui;
use std::collections::BTreeSet;

const ALWAYS_KEPT: &[&str] = &[
    "api_key",
    "gemini_api_key",
    "openrouter_api_key",
    "cerebras_api_key",
    "ui_language",
    "use_groq",
    "use_gemini",
    "use_openrouter",
    "use_cerebras",
    "use_ollama",
    "ollama_base_url",
    "custom_models",
    "restore_defaults_selection",
];
const PRESETS: &[&str] = &[
    "presets",
    "active_preset_idx",
    "preset_profiles",
    "active_preset_profile_idx",
];
const APP: &[&str] = &[
    "theme_mode",
    "max_history_items",
    "max_screen_record_projects",
    "cc_max_memory_items",
    "graphics_mode",
    "favorite_overlay_opacity",
    "start_in_tray",
    "run_as_admin_on_startup",
    "run_at_startup",
    "authorized_startup_path",
    "show_favorite_bubble",
    "favorite_bubble_position",
    "favorites_keep_open",
    "favorite_bubble_size",
];
const MODELS: &[&str] = &[
    "model_priority_chains",
    "ollama_vision_model",
    "ollama_text_model",
];
const AUDIO: &[&str] = &[
    "realtime_translation_model",
    "realtime_transcription_model",
    "realtime_transcription_language",
    "realtime_font_size",
    "realtime_transcription_size",
    "realtime_translation_size",
    "realtime_audio_source",
    "realtime_target_language",
    "tts_method",
    "tts_voice",
    "tts_speed",
    "tts_gemini_live_model",
    "tts_output_device",
    "tts_language_conditions",
    "edge_tts_settings",
    "step_audio_settings",
    "step_audio_reference_voices",
    "magpie_settings",
    "kokoro_settings",
    "supertonic_settings",
    "vieneu_settings",
    "voxtral_settings",
    "tts_playground",
];
const SHORTCUTS: &[&str] = &[
    "screen_record_hotkeys",
    "computer_control_hotkeys",
    "screen_record_window_size",
    "translation_gummy",
];
const LOCAL_DATA: &[&str] = &["clear_webview_on_startup"];

#[test]
fn every_config_field_has_one_restore_policy() {
    let classified: Vec<&str> = [
        ALWAYS_KEPT,
        PRESETS,
        APP,
        MODELS,
        AUDIO,
        SHORTCUTS,
        LOCAL_DATA,
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect();
    let unique: BTreeSet<&str> = classified.iter().copied().collect();
    assert_eq!(
        unique.len(),
        classified.len(),
        "a config field has more than one restore policy"
    );

    let serialized = serde_json::to_value(Config::default()).unwrap();
    let actual: BTreeSet<&str> = serialized
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(actual, unique, "every Config field must be classified");
}

#[test]
fn configs_without_saved_checklist_default_to_all_selected() {
    let mut serialized = serde_json::to_value(Config::default()).unwrap();
    serialized
        .as_object_mut()
        .unwrap()
        .remove("restore_defaults_selection");

    let restored: Config = serde_json::from_value(serialized).unwrap();
    assert!(restored.restore_defaults_selection.all());
}

#[test]
fn modal_fits_the_minimum_window_in_every_supported_locale() {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1245.0, 660.0));

    for language in ["en", "vi", "ko"] {
        let context = egui::Context::default();
        crate::gui::configure_fonts(&context);
        crate::gui::theme::AppTheme::apply_global_style(&context, false);
        let mut config = Config::default();
        let text = LocaleText::get(language);
        let mut show_modal = true;
        let mut run_at_startup = false;
        let auto_launcher = None::<AutoLaunch>;
        let mut observed_rects = Vec::new();

        // Anchored egui Areas learn their content size in the first frame and
        // use it for exact centering from the second frame onward.
        for frame in 0..2 {
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(frame as f64 / 60.0),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        assert!(!render_restore_defaults_modal(
                            ui,
                            &mut config,
                            &text,
                            &mut show_modal,
                            &mut run_at_startup,
                            &auto_launcher,
                        ));
                    });
                },
            );
            observed_rects.push(
                context
                    .data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new(
                            "restore_defaults_modal_test_rect",
                        ))
                    })
                    .expect("modal rect should be captured"),
            );
        }

        let modal_rect = *observed_rects.last().unwrap();
        assert!(
            screen.contains_rect(modal_rect),
            "{language} modal overflowed minimum viewport: {observed_rects:?}"
        );
        assert!(
            modal_rect.height() <= screen.height() - 40.0,
            "{language} modal left too little vertical breathing room: {observed_rects:?}"
        );
    }
}

#[test]
fn non_default_fixture_exercises_every_resettable_config_field() {
    let defaults = serde_json::to_value(Config::default()).unwrap();
    let changed = serde_json::to_value(non_default_config()).unwrap();

    for key in [PRESETS, APP, MODELS, AUDIO, SHORTCUTS]
        .into_iter()
        .flatten()
    {
        assert_ne!(
            &changed[*key], &defaults[*key],
            "test fixture must differ for {key}"
        );
    }
}

#[test]
fn each_category_resets_only_its_owned_fields() {
    for (selection, selected_keys) in [
        (only(|selection| selection.presets = true), PRESETS),
        (only(|selection| selection.app_settings = true), APP),
        (only(|selection| selection.model_settings = true), MODELS),
        (only(|selection| selection.audio_settings = true), AUDIO),
        (
            only(|selection| selection.shortcuts_and_mini_apps = true),
            SHORTCUTS,
        ),
    ] {
        let defaults = Config::default();
        let mut config = non_default_config();
        let before = serde_json::to_value(&config).unwrap();
        let expected_defaults = serde_json::to_value(&defaults).unwrap();

        apply_selected_config_defaults_from(&mut config, selection, &defaults);
        let actual = serde_json::to_value(&config).unwrap();

        for key in [PRESETS, APP, MODELS, AUDIO, SHORTCUTS]
            .into_iter()
            .flatten()
        {
            let expected = if selected_keys.contains(key) {
                &expected_defaults[key]
            } else {
                &before[key]
            };
            assert_eq!(&actual[key], expected, "unexpected policy for {key}");
        }
        for key in ALWAYS_KEPT {
            assert_eq!(&actual[*key], &before[*key], "{key} must be kept");
        }
    }
}

#[test]
fn selecting_every_category_matches_the_legacy_reset_contract() {
    let defaults = Config::default();
    let mut config = non_default_config();
    let before = serde_json::to_value(&config).unwrap();

    apply_selected_config_defaults_from(
        &mut config,
        RestoreDefaultsSelection::default(),
        &defaults,
    );
    let actual = serde_json::to_value(&config).unwrap();
    let mut expected = serde_json::to_value(defaults).unwrap();
    for key in ALWAYS_KEPT {
        expected[*key] = before[*key].clone();
    }
    expected["clear_webview_on_startup"] = true.into();

    assert_eq!(actual, expected);
}

#[test]
fn local_data_selection_only_schedules_cleanup() {
    let defaults = Config::default();
    let mut config = non_default_config();
    let before = serde_json::to_value(&config).unwrap();
    let selection = only(|selection| selection.local_data = true);

    apply_selected_config_defaults_from(&mut config, selection, &defaults);
    let actual = serde_json::to_value(&config).unwrap();

    assert_eq!(actual["clear_webview_on_startup"], true);
    for key in [ALWAYS_KEPT, PRESETS, APP, MODELS, AUDIO, SHORTCUTS]
        .into_iter()
        .flatten()
    {
        assert_eq!(&actual[*key], &before[*key], "{key} changed");
    }
}

fn only(update: impl FnOnce(&mut RestoreDefaultsSelection)) -> RestoreDefaultsSelection {
    let mut selection = RestoreDefaultsSelection::default();
    selection.set_all(false);
    update(&mut selection);
    selection
}

fn non_default_config() -> Config {
    let defaults = Config::default();
    let mut config = Config {
        api_key: "groq-secret".to_string(),
        gemini_api_key: "gemini-secret".to_string(),
        openrouter_api_key: "openrouter-secret".to_string(),
        cerebras_api_key: "cerebras-secret".to_string(),
        presets: Vec::new(),
        active_preset_idx: 7,
        preset_profiles: Vec::new(),
        active_preset_profile_idx: 3,
        theme_mode: ThemeMode::Dark,
        ui_language: "test-locale".to_string(),
        max_history_items: defaults.max_history_items + 1,
        max_screen_record_projects: defaults.max_screen_record_projects + 1,
        cc_max_memory_items: defaults.cc_max_memory_items + 1,
        graphics_mode: "minimal".to_string(),
        favorite_overlay_opacity: 42,
        start_in_tray: true,
        run_as_admin_on_startup: true,
        run_at_startup: true,
        authorized_startup_path: "other.exe".to_string(),
        use_groq: false,
        use_gemini: false,
        use_openrouter: true,
        use_cerebras: false,
        use_ollama: true,
        ollama_base_url: "http://example.test".to_string(),
        ollama_vision_model: "vision-local".to_string(),
        ollama_text_model: "text-local".to_string(),
        custom_models: vec![Default::default()],
        realtime_translation_model: "translation-model".to_string(),
        realtime_transcription_model: "transcription-model".to_string(),
        realtime_transcription_language: "fr".to_string(),
        realtime_font_size: 99,
        realtime_transcription_size: (901, 902),
        realtime_translation_size: (903, 904),
        realtime_audio_source: "mic".to_string(),
        realtime_target_language: "Korean".to_string(),
        tts_method: crate::config::TtsMethod::EdgeTTS,
        tts_voice: "changed-voice".to_string(),
        tts_speed: "Slow".to_string(),
        tts_gemini_live_model: "changed-live-model".to_string(),
        tts_output_device: "device-id".to_string(),
        tts_language_conditions: Vec::new(),
        step_audio_reference_voices: vec![Default::default()],
        show_favorite_bubble: true,
        favorite_bubble_position: Some((12, 34)),
        favorites_keep_open: true,
        favorite_bubble_size: 40,
        screen_record_hotkeys: vec![Hotkey::new(1, "A", 2)],
        computer_control_hotkeys: vec![Hotkey::new(3, "B", 4)],
        screen_record_window_size: (1111, 777),
        ..defaults
    };
    config.model_priority_chains.image_to_text.clear();
    config.model_priority_chains.text_to_text.clear();
    config.edge_tts_settings.pitch = 10;
    config.step_audio_settings.voice = "step-voice".to_string();
    config.magpie_settings.voice = "magpie-voice".to_string();
    config.kokoro_settings.voice = "kokoro-voice".to_string();
    config.supertonic_settings.speed = 1.5;
    config.vieneu_settings.emotion = "storytelling".to_string();
    config.voxtral_settings.voice = "voxtral-voice".to_string();
    config.tts_playground.draft_text = "changed draft".to_string();
    config.translation_gummy.guide_seen = true;
    config
}
