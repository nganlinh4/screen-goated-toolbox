use std::collections::BTreeMap;

use super::LocaleText;

const FIXTURE: &str = include_str!("../../../parity-fixtures/mobile-shell/ui-language-theme.json");

fn fixture_case<'a>(fixture: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    fixture["cases"]
        .as_array()
        .expect("fixture cases")
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing fixture case {name}"))
}

#[test]
fn locale_resolution_matches_shared_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("valid fixture");
    let case = fixture_case(
        &fixture,
        "locale_resolution_uses_explicit_code_and_falls_back_to_english",
    );

    for resolution in case["cases"].as_array().expect("resolution cases") {
        let input = resolution["input"].as_str().expect("input");
        let expected = resolution["expected_locale_code"]
            .as_str()
            .expect("expected locale code");
        assert_eq!(
            LocaleText::get(input).locale_code,
            expected,
            "input {input:?}"
        );
    }
}

#[test]
fn localized_preview_templates_match_shared_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("valid fixture");
    let case = fixture_case(
        &fixture,
        "localized_preview_text_comes_from_ui_language_bundle",
    );
    let voice_name = case["voice_name"].as_str().expect("voice name");
    let expected_count = case["expected_template_count"]
        .as_u64()
        .expect("template count") as usize;

    for expected in case["locales"].as_array().expect("preview locales") {
        let language = expected["ui_language"].as_str().expect("UI language");
        let prefix = expected["expected_prefix"].as_str().expect("prefix");
        let expected_first = expected["expected_first_rendered"]
            .as_str()
            .expect("rendered preview");
        let locale = LocaleText::get(language);
        assert_eq!(
            locale.tts_settings.tts_preview_texts.len(),
            expected_count,
            "{language}"
        );
        let rendered = locale.tts_settings.tts_preview_texts[0].replace("{}", voice_name);
        assert!(rendered.starts_with(prefix), "{language}: {rendered}");
        assert_eq!(rendered, expected_first, "{language}");
    }
}

fn public_field_names(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub "))
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim()))
        .collect()
}

#[test]
fn locale_root_contains_only_the_eighteen_typed_sections() {
    let LocaleText {
        locale_code: _,
        badge: _,
        workspace: _,
        preset_basics: _,
        desktop_settings: _,
        preset_editor: _,
        global_settings: _,
        tts_playground: _,
        model_catalog: _,
        tts_settings: _,
        tts_advanced: _,
        realtime: _,
        live_translate: _,
        screen_translate: _,
        shell: _,
        translation_gummy: _,
        tool_runtime: _,
        overlay: _,
        auxiliary: _,
    } = LocaleText::get("en");

    assert_eq!(
        public_field_names(include_str!("text.rs")),
        [
            "locale_code",
            "badge",
            "workspace",
            "preset_basics",
            "desktop_settings",
            "preset_editor",
            "global_settings",
            "tts_playground",
            "model_catalog",
            "tts_settings",
            "tts_advanced",
            "realtime",
            "live_translate",
            "screen_translate",
            "shell",
            "translation_gummy",
            "tool_runtime",
            "overlay",
            "auxiliary",
        ]
    );
}

#[test]
fn locale_leaf_fields_have_one_section_owner() {
    let sections = [
        ("badge", include_str!("badge.rs"), 47),
        ("workspace", include_str!("workspace.rs"), 25),
        ("preset_basics", include_str!("preset_basics.rs"), 36),
        ("desktop_settings", include_str!("desktop_settings.rs"), 38),
        ("preset_editor", include_str!("preset_editor.rs"), 37),
        ("global_settings", include_str!("global_settings.rs"), 34),
        ("tts_playground", include_str!("tts_playground.rs"), 29),
        ("model_catalog", include_str!("model_catalog.rs"), 28),
        ("tts_settings", include_str!("tts_settings.rs"), 29),
        ("tts_advanced", include_str!("tts_advanced.rs"), 35),
        ("realtime", include_str!("realtime.rs"), 32),
        ("live_translate", include_str!("live_translate.rs"), 11),
        ("screen_translate", include_str!("screen_translate.rs"), 19),
        ("shell", include_str!("shell.rs"), 47),
        (
            "translation_gummy",
            include_str!("translation_gummy.rs"),
            30,
        ),
        ("tool_runtime", include_str!("tool_runtime.rs"), 44),
        ("overlay", include_str!("overlay.rs"), 22),
        ("auxiliary", include_str!("auxiliary.rs"), 10),
    ];
    let mut owners = BTreeMap::new();

    for (section, source, expected_count) in sections {
        let fields = public_field_names(source);
        assert_eq!(fields.len(), expected_count, "{section}");
        for field in fields {
            assert!(
                owners.insert(field, section).is_none(),
                "{field} has more than one section owner"
            );
        }
    }

    assert_eq!(owners.len(), 553);
    assert_eq!(owners["cancel_label"], "preset_basics");
    assert_eq!(owners["favorites_keep_open"], "shell");
    assert_eq!(owners["image_creator_btn"], "shell");
    assert_eq!(owners["download"], "auxiliary");
    assert_eq!(owners["managed_tools"], "auxiliary");
    assert_eq!(owners["realtime_app_loading"], "realtime");
    assert_eq!(owners["live_translate_title"], "live_translate");
}

#[test]
fn downloaded_tools_progress_and_cards_are_localized() {
    assert_eq!(
        public_field_names(include_str!("managed_tools.rs")).len(),
        149
    );
    let english = LocaleText::get("en");
    for code in ["en", "ko", "vi"] {
        let locale = LocaleText::get(code);
        let badge = &locale.badge;
        for template in [
            badge.downloading_component_fmt,
            badge.preparing_component_fmt,
            badge.checking_component_fmt,
            badge.finalizing_component_fmt,
            badge.component_installed_fmt,
            badge.component_install_failed_fmt,
            badge.removing_component_fmt,
            badge.component_removed_fmt,
            badge.component_remove_failed_fmt,
        ] {
            assert!(template.contains("{name}"), "{code}: {template}");
        }
        let managed = &locale.auxiliary.managed_tools;
        for value in [
            managed.tool_screen_recorder_card,
            managed.tool_computer_control_card,
            managed.tool_screen_translate_card,
            managed.tool_screen_translate_detector,
            managed.tool_desc_screen_translate_detector,
            managed.tool_creation_interface,
            managed.tool_prompt_dj_interface,
            managed.tool_tts_playground_interface,
            managed.tool_step_audio_model,
            managed.tool_magpie_model,
            managed.tool_vieneu_model,
            managed.downloaded_tools_clean_confirm_title,
            managed.downloaded_tools_clean_confirm_body,
            managed.downloaded_tools_clean_progress_title,
            managed.downloaded_tools_clean_complete,
            managed.tool_desc_supertonic,
            managed.tool_mcp_time_desc,
            managed.tool_mcp_blender_desc,
        ] {
            assert!(!value.trim().is_empty(), "{code}");
        }
    }
    for code in ["ko", "vi"] {
        let localized = LocaleText::get(code);
        assert_ne!(
            localized
                .auxiliary
                .managed_tools
                .tool_screen_recorder_payload,
            english.auxiliary.managed_tools.tool_screen_recorder_payload
        );
        assert_ne!(
            localized.badge.preparing_component_fmt,
            english.badge.preparing_component_fmt
        );
        assert_ne!(
            localized
                .auxiliary
                .managed_tools
                .downloaded_tools_clean_confirm_body,
            english
                .auxiliary
                .managed_tools
                .downloaded_tools_clean_confirm_body
        );
        assert_ne!(
            localized.auxiliary.managed_tools.tool_desc_supertonic,
            english.auxiliary.managed_tools.tool_desc_supertonic
        );
    }
}

#[test]
fn hotkey_conflicts_are_localized_from_structured_owners() {
    use crate::config::{GlobalHotkeyOwner, HotkeyConflict};

    let global = HotkeyConflict::Global {
        owner: GlobalHotkeyOwner::ComputerControl,
        hotkey_name: "Ctrl + F6".to_string(),
    };
    let preset = HotkeyConflict::Preset {
        hotkey_name: "Ctrl + F7".to_string(),
        preset_name: "Work".to_string(),
    };

    assert_eq!(
        LocaleText::get("en").hotkey_conflict_message(&global),
        "Hotkey 'Ctrl + F6' conflicts with Computer Control."
    );
    assert_eq!(
        LocaleText::get("ko").hotkey_conflict_message(&global),
        "'Ctrl + F6' 단축키가 컴퓨터 제어 단축키와 충돌합니다."
    );
    assert_eq!(
        LocaleText::get("vi").hotkey_conflict_message(&preset),
        "Phím 'Ctrl + F7' xung đột với cấu hình 'Work'."
    );
}
