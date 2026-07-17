//! Derive a bounded public domain scope from exact research source URLs.

use serde_json::Value;

pub(super) fn from_source_urls(
    args: &Value,
    max_urls: usize,
    max_url_chars: usize,
) -> Option<Vec<String>> {
    let urls = args.get("source_urls")?.as_array()?;
    if urls.is_empty() || urls.len() > max_urls {
        return None;
    }
    let mut domains = Vec::new();
    for value in urls {
        let raw = value.as_str()?.trim();
        if raw.chars().count() > max_url_chars {
            return None;
        }
        let parsed = url::Url::parse(raw).ok()?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return None;
        }
        let host = parsed.domain()?.trim_end_matches('.').to_ascii_lowercase();
        let domain = psl2::registrable_domain(&host)?;
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    Some(domains)
}
