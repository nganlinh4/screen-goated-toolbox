//! Validation for optional caller-supplied public research sources.

use serde_json::{Value, json};
use std::collections::HashSet;

use super::source_policy::SourcePolicy;

const MAX_DIRECT_SOURCES: usize = 5;
const MAX_DIRECT_URL_CHARS: usize = 2_048;

pub(super) fn parse(
    args: &Value,
    policy: &SourcePolicy,
    max_sources: usize,
) -> Result<Vec<String>, Value> {
    let Some(raw_urls) = args.get("source_urls") else {
        return Ok(Vec::new());
    };
    let Some(raw_urls) = raw_urls.as_array() else {
        return Err(argument_error(
            "source_urls must be an array of public HTTP URLs",
        ));
    };
    if raw_urls.is_empty() || raw_urls.len() > MAX_DIRECT_SOURCES || raw_urls.len() > max_sources {
        return Err(argument_error(
            "source_urls must contain between 1 and max_sources URLs",
        ));
    }

    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for value in raw_urls {
        let Some(raw) = value.as_str() else {
            return Err(argument_error("source_urls entries must be strings"));
        };
        let Some(url) = public_source_url(raw) else {
            return Err(argument_error(
                "source_urls entries must be bounded public HTTP URLs without credentials",
            ));
        };
        if !policy.accepts(&url) {
            return Err(argument_error(
                "source_urls entries must satisfy the selected source policy",
            ));
        }
        if seen.insert(url.clone()) {
            urls.push(url);
        }
    }
    Ok(urls)
}

fn public_source_url(raw: &str) -> Option<String> {
    if raw.chars().count() > MAX_DIRECT_URL_CHARS {
        return None;
    }
    let mut parsed = url::Url::parse(raw.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    let host = parsed.domain()?.trim_end_matches('.').to_ascii_lowercase();
    psl2::registrable_domain(&host)?;
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

fn argument_error(error: &str) -> Value {
    json!({
        "ok": false,
        "code": "ERR_RESEARCH_BAD_ARGUMENT",
        "error": error,
        "effect_may_have_occurred": false,
        "executed": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restricted_policy() -> SourcePolicy {
        SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["example.com"],
            }),
            2,
        )
        .unwrap()
    }

    #[test]
    fn direct_sources_are_public_policy_bound_and_deduplicated() {
        let urls = parse(
            &json!({
                "source_urls": [
                    "https://docs.example.com/fact#section",
                    "https://docs.example.com/fact",
                ],
            }),
            &restricted_policy(),
            2,
        )
        .unwrap();
        assert_eq!(urls, ["https://docs.example.com/fact"]);
    }

    #[test]
    fn direct_sources_reject_private_credentials_and_out_of_policy_hosts() {
        for url in [
            "http://localhost:8080/private",
            "https://user:secret@example.com/fact",
            "https://outside.test/fact",
        ] {
            assert!(
                parse(&json!({"source_urls": [url]}), &restricted_policy(), 2,).is_err(),
                "unsafe direct source accepted: {url}"
            );
        }
    }
}
