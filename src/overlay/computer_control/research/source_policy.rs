//! Structural source-selection contracts for browser-backed research.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};

#[path = "source_policy_domains.rs"]
mod source_policy_domains;

const MAX_ALLOWED_DOMAINS: usize = 5;
const MAX_SAFE_OUTPUT_URL_CHARS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    BestAvailable,
    Broad,
    DomainRestricted,
}

pub(super) struct SourcePolicy {
    kind: Kind,
    allowed_domains: Vec<String>,
}

pub(super) struct Coverage {
    pub assessed: bool,
    pub covered: Vec<String>,
    pub missing: Vec<String>,
}

impl SourcePolicy {
    pub(super) fn parse(args: &Value, max_sources: usize) -> Result<Self, Value> {
        let name = args
            .get("source_policy")
            .and_then(Value::as_str)
            .unwrap_or("best_available");
        let kind = match name {
            "best_available" => Kind::BestAvailable,
            "broad" => Kind::Broad,
            "domain_restricted" => Kind::DomainRestricted,
            _ => {
                return Err(argument_error(
                    "source_policy must be best_available, broad, or domain_restricted",
                ));
            }
        };
        let allowed = args.get("allowed_domains");
        if kind != Kind::DomainRestricted {
            if allowed.is_some() {
                return Err(argument_error(
                    "allowed_domains is valid only with source_policy domain_restricted",
                ));
            }
            return Ok(Self {
                kind,
                allowed_domains: Vec::new(),
            });
        }
        let raw_domains = match allowed {
            Some(Value::Array(domains)) => domains,
            Some(_) => {
                return Err(argument_error(
                    "allowed_domains must be an array of host names",
                ));
            }
            None => {
                let Some(domains) = source_policy_domains::from_source_urls(
                    args,
                    MAX_ALLOWED_DOMAINS,
                    MAX_SAFE_OUTPUT_URL_CHARS,
                ) else {
                    return Err(argument_error(
                        "domain_restricted research needs allowed_domains or valid public source_urls",
                    ));
                };
                if domains.len() > max_sources {
                    return Err(argument_error(
                        "max_sources must be at least the number of distinct source domains",
                    ));
                }
                return Ok(Self {
                    kind,
                    allowed_domains: domains,
                });
            }
        };
        if raw_domains.is_empty() || raw_domains.len() > MAX_ALLOWED_DOMAINS {
            return Err(argument_error(
                "allowed_domains must contain between 1 and 5 host names",
            ));
        }
        let mut domains = Vec::with_capacity(raw_domains.len());
        for value in raw_domains {
            let Some(raw) = value.as_str() else {
                return Err(argument_error("allowed_domains entries must be strings"));
            };
            let Some(domain) = normalize_domain(raw) else {
                return Err(argument_error(
                    "allowed_domains entries must be sufficiently narrow bare public host names",
                ));
            };
            if domains.contains(&domain) {
                continue;
            }
            if domains
                .iter()
                .any(|existing| domains_overlap(existing, &domain))
            {
                return Err(argument_error(
                    "allowed_domains must not contain overlapping parent and child host scopes",
                ));
            }
            domains.push(domain);
        }
        if domains.len() > max_sources {
            return Err(argument_error(
                "max_sources must be at least the number of distinct allowed_domains",
            ));
        }
        Ok(Self {
            kind,
            allowed_domains: domains,
        })
    }

    pub(super) fn name(&self) -> &'static str {
        match self.kind {
            Kind::BestAvailable => "best_available",
            Kind::Broad => "broad",
            Kind::DomainRestricted => "domain_restricted",
        }
    }

    pub(super) fn search_query(&self, query: &str) -> String {
        if self.kind != Kind::DomainRestricted {
            return query.to_string();
        }
        let sites = self
            .allowed_domains
            .iter()
            .map(|domain| format!("site:{domain}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        format!("{query} ({sites})")
    }

    pub(super) fn relevance_identity_terms(&self) -> &[String] {
        &self.allowed_domains
    }

    pub(super) fn missing_domain_search_queries<'a>(
        &self,
        query: &str,
        discovered_urls: impl Iterator<Item = &'a str>,
    ) -> Vec<String> {
        self.coverage(discovered_urls)
            .missing
            .into_iter()
            .map(|domain| format!("{query} site:{domain}"))
            .collect()
    }

    pub(super) fn select_candidates(&self, urls: Vec<String>, limit: usize) -> Vec<String> {
        let urls = canonical_unique_urls(urls)
            .into_iter()
            .filter(|url| self.accepts(url))
            .collect::<Vec<_>>();
        match self.kind {
            Kind::Broad => urls.into_iter().take(limit).collect(),
            Kind::BestAvailable => diversify(urls, limit, host_key),
            Kind::DomainRestricted => {
                let domain_order = self.allowed_domains.clone();
                diversify(urls, limit, |url| {
                    matching_domain(url, &domain_order).unwrap_or_default()
                })
            }
        }
    }

    pub(super) fn accepts(&self, url: &str) -> bool {
        let Some(host) = parsed_host(url) else {
            return false;
        };
        self.kind != Kind::DomainRestricted
            || self
                .allowed_domains
                .iter()
                .any(|domain| host_matches(&host, domain))
    }

    pub(super) fn coverage<'a>(&self, urls: impl Iterator<Item = &'a str>) -> Coverage {
        if self.kind != Kind::DomainRestricted {
            return Coverage {
                assessed: false,
                covered: Vec::new(),
                missing: Vec::new(),
            };
        }
        let mut seen = HashSet::new();
        for url in urls {
            if let Some(domain) = matching_domain(url, &self.allowed_domains) {
                seen.insert(domain);
            }
        }
        let covered = self
            .allowed_domains
            .iter()
            .filter(|domain| seen.contains(*domain))
            .cloned()
            .collect();
        let missing = self
            .allowed_domains
            .iter()
            .filter(|domain| !seen.contains(*domain))
            .cloned()
            .collect();
        Coverage {
            assessed: true,
            covered,
            missing,
        }
    }

    pub(super) fn requested_domain(&self, url: &str) -> Option<String> {
        (self.kind == Kind::DomainRestricted)
            .then(|| matching_domain(url, &self.allowed_domains))
            .flatten()
    }

    pub(super) fn accepts_redirect(&self, requested: &str, final_url: &str) -> bool {
        self.accepts(final_url) && same_registrable_domain(requested, final_url)
    }
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

fn normalize_domain(raw: &str) -> Option<String> {
    let value = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 253
        || value.starts_with('.')
        || value.starts_with("*.")
        || value.contains(['/', '?', '#', '@', ':'])
        || value.chars().any(char::is_whitespace)
    {
        return None;
    }
    let parsed = url::Url::parse(&format!("https://{value}/")).ok()?;
    let domain = parsed.domain()?.trim_end_matches('.').to_ascii_lowercase();
    (domain == value && psl2::registrable_domain(&domain).is_some()).then_some(domain)
}

fn domains_overlap(left: &str, right: &str) -> bool {
    host_matches(left, right) || host_matches(right, left)
}

fn canonical_unique_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    urls.into_iter()
        .filter_map(|raw| {
            let mut parsed = url::Url::parse(&raw).ok()?;
            if !matches!(parsed.scheme(), "http" | "https")
                || parsed.domain().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return None;
            }
            parsed.set_fragment(None);
            let canonical = parsed.to_string();
            seen.insert(canonical.clone()).then_some(canonical)
        })
        .collect()
}

pub(super) fn canonical_url_key(raw: &str) -> Option<String> {
    canonical_unique_urls(vec![raw.to_string()])
        .into_iter()
        .next()
}

pub(super) fn safe_url_for_output(raw: &str) -> Option<(String, bool)> {
    let mut parsed = url::Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.domain().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    let query_omitted = parsed.query().is_some();
    parsed.set_query(None);
    parsed.set_fragment(None);
    let safe = parsed.to_string();
    (safe.chars().count() <= MAX_SAFE_OUTPUT_URL_CHARS).then_some((safe, query_omitted))
}

fn diversify(urls: Vec<String>, limit: usize, key: impl Fn(&str) -> String) -> Vec<String> {
    let mut order = Vec::new();
    let mut groups: HashMap<String, VecDeque<String>> = HashMap::new();
    for url in urls {
        let group = key(&url);
        if !groups.contains_key(&group) {
            order.push(group.clone());
        }
        groups.entry(group).or_default().push_back(url);
    }
    let mut selected = Vec::new();
    while selected.len() < limit {
        let mut progressed = false;
        for group in &order {
            if let Some(url) = groups.get_mut(group).and_then(VecDeque::pop_front) {
                selected.push(url);
                progressed = true;
                if selected.len() == limit {
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
    }
    selected
}

fn host_key(url: &str) -> String {
    parsed_host(url).unwrap_or_default()
}

fn matching_domain(url: &str, allowed_domains: &[String]) -> Option<String> {
    let host = parsed_host(url)?;
    allowed_domains
        .iter()
        .filter(|domain| host_matches(&host, domain))
        .max_by_key(|domain| (domain.split('.').count(), domain.len()))
        .cloned()
}

fn parsed_host(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .domain()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn same_registrable_domain(left: &str, right: &str) -> bool {
    let Some(left) = parsed_host(left).and_then(|host| psl2::registrable_domain(&host)) else {
        return false;
    };
    let Some(right) = parsed_host(right).and_then(|host| psl2::registrable_domain(&host)) else {
        return false;
    };
    left.eq_ignore_ascii_case(&right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_policy_requires_explicit_valid_hosts_and_capacity() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["Example.COM.", "docs.example.org"],
            }),
            2,
        )
        .unwrap();
        assert_eq!(policy.allowed_domains, ["example.com", "docs.example.org"]);
        assert!(policy.search_query("facts").contains("site:example.com"));
        assert!(SourcePolicy::parse(
            &json!({"source_policy": "domain_restricted", "allowed_domains": ["https://example.com"]}),
            1,
        )
        .is_err());
        assert!(SourcePolicy::parse(
            &json!({"source_policy": "domain_restricted", "allowed_domains": ["a.test", "b.test"]}),
            1,
        )
        .is_err());
    }

    #[test]
    fn exact_public_sources_can_define_the_restricted_domain_scope() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "source_urls": [
                    "https://docs.example.com/fact",
                    "https://api.example.com/reference",
                ],
            }),
            2,
        )
        .unwrap();
        assert_eq!(policy.allowed_domains, ["example.com"]);
        assert!(policy.accepts("https://help.example.com/related"));
        assert!(policy.search_query("facts").contains("site:example.com"));

        for source_url in [
            "http://localhost/private",
            "https://user:secret@example.com/private",
        ] {
            assert!(
                SourcePolicy::parse(
                    &json!({
                        "source_policy": "domain_restricted",
                        "source_urls": [source_url],
                    }),
                    1,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn restricted_policy_rejects_registry_wide_scopes() {
        for domain in ["com", "co.uk", "COM.", "org.au"] {
            assert!(
                SourcePolicy::parse(
                    &json!({
                        "source_policy": "domain_restricted",
                        "allowed_domains": [domain],
                    }),
                    1,
                )
                .is_err(),
                "dangerously broad scope was accepted: {domain}"
            );
        }
        for domain in ["example.co.uk", "example.ai", "example.org.au"] {
            assert!(
                SourcePolicy::parse(
                    &json!({
                        "source_policy": "domain_restricted",
                        "allowed_domains": [domain],
                    }),
                    1,
                )
                .is_ok(),
                "registrable scope was rejected: {domain}"
            );
        }
    }

    #[test]
    fn restricted_policy_rejects_overlapping_scopes_in_either_order() {
        for domains in [
            ["example.com", "docs.example.com"],
            ["docs.example.com", "example.com"],
        ] {
            assert!(
                SourcePolicy::parse(
                    &json!({
                        "source_policy": "domain_restricted",
                        "allowed_domains": domains,
                    }),
                    2,
                )
                .is_err()
            );
        }

        let allowed = vec!["example.com".to_string(), "docs.example.com".to_string()];
        assert_eq!(
            matching_domain("https://api.docs.example.com/reference", &allowed).as_deref(),
            Some("docs.example.com")
        );
    }

    #[test]
    fn model_visible_urls_drop_queries_and_reject_userinfo() {
        let (safe, query_omitted) =
            safe_url_for_output("https://docs.example.com/path?token=secret&item=7#section")
                .unwrap();
        assert_eq!(safe, "https://docs.example.com/path");
        assert!(query_omitted);
        assert!(safe_url_for_output("https://user:pass@example.com/path").is_none());
        assert!(canonical_url_key("https://user:pass@example.com/path").is_none());
        let oversized = format!("https://example.com/{}", "segment/".repeat(400));
        assert!(safe_url_for_output(&oversized).is_none());
    }

    #[test]
    fn restricted_selection_is_hard_filtered_deduplicated_and_balanced() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["one.test", "two.test"],
            }),
            4,
        )
        .unwrap();
        let selected = policy.select_candidates(
            vec![
                "https://one.test/a#top".into(),
                "https://one.test/a".into(),
                "https://one.test/b".into(),
                "https://evil-one.test/x".into(),
                "https://docs.two.test/c".into(),
            ],
            4,
        );
        assert_eq!(selected.len(), 3);
        assert_eq!(parsed_host(&selected[0]).as_deref(), Some("one.test"));
        assert_eq!(parsed_host(&selected[1]).as_deref(), Some("docs.two.test"));
        assert!(selected.iter().all(|url| policy.accepts(url)));
        assert!(!policy.accepts("https://one.test.evil.invalid/a"));
        assert!(!policy.accepts("https://evil-one.test/a"));
    }

    #[test]
    fn restricted_coverage_reports_each_requested_domain() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["one.test", "two.test"],
            }),
            2,
        )
        .unwrap();
        let coverage = policy.coverage(["https://help.one.test/a"].into_iter());
        assert!(coverage.assessed);
        assert_eq!(coverage.covered, ["one.test"]);
        assert_eq!(coverage.missing, ["two.test"]);
    }

    #[test]
    fn restricted_discovery_retries_only_domains_missing_from_results() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["one.test", "two.test"]
            }),
            2,
        )
        .unwrap();
        assert_eq!(
            policy.missing_domain_search_queries("facts", ["https://help.one.test/a"].into_iter()),
            ["facts site:two.test"]
        );
        assert_eq!(
            policy.missing_domain_search_queries("facts", std::iter::empty()),
            ["facts site:one.test", "facts site:two.test"]
        );
    }

    #[test]
    fn source_redirects_stay_with_the_discovered_site_identity() {
        let broad = SourcePolicy::parse(&json!({"source_policy": "broad"}), 2).unwrap();
        assert!(broad.accepts_redirect(
            "https://www.example.co.uk/start",
            "https://help.example.co.uk/final"
        ));
        assert!(!broad.accepts_redirect(
            "https://example.co.uk/start",
            "https://accounts.other.test/final"
        ));
    }

    #[test]
    fn best_available_diversifies_hosts_while_broad_preserves_order() {
        let urls = vec![
            "https://one.test/a".into(),
            "https://one.test/b".into(),
            "https://two.test/a".into(),
        ];
        let best = SourcePolicy::parse(&json!({}), 3).unwrap();
        assert_eq!(
            best.select_candidates(urls.clone(), 3),
            [
                "https://one.test/a".to_string(),
                "https://two.test/a".to_string(),
                "https://one.test/b".to_string(),
            ]
        );
        let broad = SourcePolicy::parse(&json!({"source_policy": "broad"}), 3).unwrap();
        assert_eq!(broad.select_candidates(urls, 3)[1], "https://one.test/b");
    }
}
