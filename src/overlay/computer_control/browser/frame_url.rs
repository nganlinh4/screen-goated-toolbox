//! Reconstruct the complete URL from the CDP frame's split URL fields.

use anyhow::{Result, anyhow};
use serde_json::Value;

pub(super) fn from_frame(frame: &Value) -> Result<String> {
    let base = frame
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("browser main frame omitted its URL"))?;
    let mut result = base.to_owned();
    if let Some(fragment) = frame.get("urlFragment") {
        let fragment = fragment
            .as_str()
            .filter(|value| value.starts_with('#'))
            .ok_or_else(|| anyhow!("browser main frame returned an invalid URL fragment"))?;
        if base.contains('#') {
            anyhow::bail!("browser main frame returned conflicting URL fragments");
        }
        result.push_str(fragment);
    }
    Ok(result)
}

pub(super) fn equivalent(left: &str, right: &str) -> bool {
    left == right
        || matches!((url::Url::parse(left), url::Url::parse(right)), (Ok(left), Ok(right)) if left == right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reconstructs_fragments_without_erasing_destination_identity() {
        assert_eq!(
            from_frame(&json!({"url":"https://example.test/","urlFragment":"#one"})).unwrap(),
            "https://example.test/#one"
        );
        assert_eq!(
            from_frame(&json!({"url":"https://example.test/","urlFragment":"#"})).unwrap(),
            "https://example.test/#"
        );
        assert_eq!(
            from_frame(&json!({"url":"https://example.test/"})).unwrap(),
            "https://example.test/"
        );
        assert!(!equivalent(
            "https://example.test/#one",
            "https://example.test/#two"
        ));
        assert!(equivalent("https://example.test", "https://example.test/"));
        assert!(from_frame(&json!({"url":"https://example.test/","urlFragment":"bad"})).is_err());
    }
}
