use super::*;
use serde_json::{Value, json};

#[test]
fn shared_onboarding_dismissal_survives_applying_an_older_draft() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/translation-gummy/onboarding-contract.json"
    ))
    .unwrap();
    let draft = TranslationGummySettings::default();
    let mut current = draft.clone();
    let flag = fixture["persistedFlag"].as_str().unwrap();
    let initial = serde_json::to_value(&draft).unwrap();
    assert_eq!(initial[flag], fixture["showWhen"][flag]);
    current.guide_seen = fixture["dismissBehavior"]["persistGuideSeen"]
        .as_bool()
        .unwrap();
    current.hotkeys = vec![crate::config::Hotkey::new(0x41, 2)];
    let applied = settings_for_apply(&draft, &current);
    assert_eq!(applied.hotkeys, current.hotkeys);
    let encoded = serde_json::to_value(&applied).unwrap();
    assert_eq!(
        encoded[flag],
        fixture["dismissBehavior"]["suppressFutureAutoShow"]
    );
    let reloaded: TranslationGummySettings = serde_json::from_value(encoded).unwrap();
    assert!(reloaded.guide_seen);
    assert_eq!(settings_for_apply(&reloaded, &current), reloaded);
}

#[test]
fn shared_state_apply_preserves_rows_and_only_allocates_a_needed_separator() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/translation-gummy/state-contract.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut ui = state::UiState {
            applied: TranslationGummySettings::default(),
            draft: TranslationGummySettings::default(),
            dirty: false,
            is_running: false,
            connection_state: TranslationGummyConnectionState::Stopped,
            transcripts: case["initial"]["transcripts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| TranslationGummyTranscriptItem {
                    id: item["id"].as_u64().unwrap(),
                    role: match item["role"].as_str().unwrap() {
                        "INPUT" => "input",
                        "OUTPUT" => "output",
                        "SEPARATOR" => "separator",
                        other => panic!("unknown fixture role {other}"),
                    },
                    text: item["text"].as_str().unwrap().to_string(),
                    is_final: item["isFinal"].as_bool().unwrap(),
                    lang: item["lang"].as_str().unwrap().to_string(),
                })
                .collect(),
            last_error: None,
            hotkey_error: None,
            audio_level: 0.0,
        };
        let allocations = std::cell::Cell::new(0);
        ui.insert_session_separator(
            || {
                allocations.set(allocations.get() + 1);
                case["apply"]["nextSeparatorId"].as_u64().unwrap()
            },
            || case["apply"]["separatorText"].as_str().unwrap().to_string(),
        );
        let project = |item: &Value| {
            json!({
                "id": item["id"], "role": item["role"].as_str().unwrap().to_ascii_lowercase(),
                "text": item["text"], "isFinal": item["isFinal"], "lang": item["lang"],
            })
        };
        let expected: Vec<_> = case["expected"]["transcripts"]
            .as_array()
            .unwrap()
            .iter()
            .map(project)
            .collect();
        let actual: Vec<_> = ui.transcripts.iter().map(|item| json!({
            "id": item.id, "role": item.role, "text": item.text, "isFinal": item.is_final, "lang": item.lang,
        })).collect();
        assert_eq!(actual, expected, "{}", case["name"]);
        assert_eq!(
            allocations.get(),
            expected.len() - case["initial"]["transcripts"].as_array().unwrap().len()
        );
    }
}
