#[derive(Debug, PartialEq, Eq)]
pub(super) enum StartEffect {
    LocalError(String),
    GlobalNotice(String),
}

pub(super) fn admit_start(
    credential_present: bool,
    message: &str,
    mut emit: impl FnMut(StartEffect),
) -> bool {
    if credential_present {
        return true;
    }
    emit(StartEffect::LocalError(message.to_string()));
    emit(StartEffect::GlobalNotice(message.to_string()));
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_startup_missing_credential_publishes_local_and_global_effects() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/api-key-notifications/triggers.json"
        ))
        .unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["surface"] == "translation_gummy_start")
            .unwrap();
        for language in ["en", "vi", "ko"] {
            let locale = crate::gui::locale::LocaleText::get(language);
            let message = locale.translation_gummy.translation_gummy_api_key_required;
            if language == "en" {
                assert_eq!(message, case["raw_error"].as_str().unwrap());
                assert_eq!(message, case["expected_en"].as_str().unwrap());
            }
            let mut effects = Vec::new();
            assert!(!admit_start(false, message, |effect| effects.push(effect)));
            assert_eq!(
                effects,
                vec![
                    StartEffect::LocalError(message.to_string()),
                    StartEffect::GlobalNotice(message.to_string())
                ]
            );
            assert_eq!(case["expected_notice"], true);
            effects.clear();
            assert!(admit_start(true, message, |effect| effects.push(effect)));
            assert!(effects.is_empty());
        }
    }
}
