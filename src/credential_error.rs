fn provider_key(error: &str) -> Option<&str> {
    ["NO_API_KEY:", "INVALID_API_KEY:"]
        .into_iter()
        .find_map(|marker| {
            let (_, tail) = error.split_once(marker)?;
            tail.trim_start()
                .split(|value: char| !value.is_alphanumeric() && !matches!(value, '_' | '-' | '.'))
                .next()
                .filter(|value| !value.is_empty())
        })
}

pub(crate) fn provider_name(error: &str) -> String {
    provider_key(error).map_or_else(
        || "API".to_string(),
        |key| {
            let key = key.to_ascii_lowercase();
            crate::model_config::provider_full_name(if key == "gemini" { "google" } else { &key })
                .to_string()
        },
    )
}

pub(crate) fn message(error: &str, language: &str) -> Option<String> {
    let missing = error.contains("NO_API_KEY");
    if !missing && !error.contains("INVALID_API_KEY") {
        return None;
    }
    let provider = provider_name(error);
    let named = provider_key(error).is_some();
    let credential = if named {
        format!("{provider} API")
    } else {
        "API".to_string()
    };
    let article = if named { "a" } else { "an" };
    Some(match (missing, language) {
        (true, "vi") => format!("Bạn chưa nhập {credential} key!"),
        (true, "ko") => format!("{credential} 키를 입력하지 않았습니다!"),
        (true, "ja") => format!("{credential}キーが入力されていません!"),
        (true, "zh") => format!("您还没有输入 {credential} key!"),
        (true, _) => format!("You haven't entered {article} {credential} key!"),
        (false, "vi") => format!("{credential} key không hợp lệ!"),
        (false, "ko") => format!("{credential} 키가 유효하지 않습니다!"),
        (false, "ja") => format!("{credential}キーが無効です!"),
        (false, "zh") => format!("{credential} key 无效!"),
        (false, _) => format!("Invalid {credential} key!"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_parity_credential_fixture_runs_on_host_and_worker() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../parity-fixtures/api-key-notifications/triggers.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let error = case["raw_error"].as_str().unwrap();
            if !error.contains("API_KEY") {
                continue;
            }
            assert_eq!(
                message(error, "en").as_deref(),
                case["expected_en"].as_str(),
                "{}",
                case["name"]
            );
            assert_eq!(
                provider_name(error),
                case["expected_provider"].as_str().unwrap()
            );
        }
        assert_eq!(message("network failed", "en"), None);
    }
}
