//! Query-relevance ranking for bounded research discovery links.

use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiscoveredLink {
    pub(super) url: String,
    pub(super) label: String,
    direct: bool,
}

impl DiscoveredLink {
    pub(super) fn search(url: String, label: String) -> Self {
        Self {
            url,
            label: bounded_label(label),
            direct: false,
        }
    }

    pub(super) fn direct(url: String) -> Self {
        Self {
            url,
            label: String::new(),
            direct: true,
        }
    }
}

pub(super) fn rank(
    query: &str,
    ignored_identities: &[String],
    links: Vec<DiscoveredLink>,
) -> Vec<DiscoveredLink> {
    let query_tokens = factual_query_tokens(query, ignored_identities);
    let mut scored = links
        .into_iter()
        .enumerate()
        .map(|(index, link)| {
            let score = relevance_score(&query_tokens, &link);
            (link.direct, score, index, link)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.relevance.cmp(&left.1.relevance))
            .then_with(|| right.1.matched_tokens.cmp(&left.1.matched_tokens))
            .then_with(|| right.1.path_matches.cmp(&left.1.path_matches))
            .then_with(|| left.1.path_segments.cmp(&right.1.path_segments))
            .then_with(|| left.1.path_chars.cmp(&right.1.path_chars))
            .then_with(|| left.2.cmp(&right.2))
    });
    scored.into_iter().map(|(_, _, _, link)| link).collect()
}

pub(super) fn relevant(
    query: &str,
    ignored_identities: &[String],
    links: Vec<DiscoveredLink>,
) -> Vec<DiscoveredLink> {
    let query_tokens = factual_query_tokens(query, ignored_identities);
    let mut scored = links
        .into_iter()
        .enumerate()
        .filter_map(|(index, link)| {
            let score = relevance_score(&query_tokens, &link);
            (score.relevance > 0).then_some((score, index, link))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .relevance
            .cmp(&left.0.relevance)
            .then_with(|| right.0.matched_tokens.cmp(&left.0.matched_tokens))
            .then_with(|| right.0.path_matches.cmp(&left.0.path_matches))
            .then_with(|| left.0.path_segments.cmp(&right.0.path_segments))
            .then_with(|| left.0.path_chars.cmp(&right.0.path_chars))
            .then_with(|| left.1.cmp(&right.1))
    });
    scored.into_iter().map(|(_, _, link)| link).collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RelevanceScore {
    relevance: usize,
    matched_tokens: usize,
    path_matches: usize,
    path_segments: usize,
    path_chars: usize,
}

fn relevance_score(query_tokens: &HashSet<String>, link: &DiscoveredLink) -> RelevanceScore {
    if query_tokens.is_empty() {
        return RelevanceScore::default();
    }
    let label_tokens = tokens(&link.label);
    let parsed = url::Url::parse(&link.url).ok();
    let identity_tokens = parsed
        .as_ref()
        .and_then(url::Url::host_str)
        .map(tokens)
        .unwrap_or_default();
    let (location, path_segments, path_chars) = parsed
        .map(|url| {
            let mut value = url.path().to_string();
            let path_segments = url.path_segments().map(Iterator::count).unwrap_or(0);
            let path_chars = url.path().chars().count();
            if let Some(query) = url.query() {
                value.push(' ');
                value.push_str(query);
            }
            (value, path_segments, path_chars)
        })
        .unwrap_or_default();
    let location_tokens = tokens(&location);
    let mut score = RelevanceScore {
        path_segments,
        path_chars,
        ..RelevanceScore::default()
    };
    for token in query_tokens
        .iter()
        .filter(|token| !identity_tokens.contains(*token))
    {
        let label_match = label_tokens.contains(token);
        let path_match = location_tokens.contains(token);
        score.relevance += usize::from(label_match) * 8 + usize::from(path_match) * 5;
        score.matched_tokens += usize::from(label_match || path_match);
        score.path_matches += usize::from(path_match);
    }
    score
}

fn factual_query_tokens(query: &str, ignored_identities: &[String]) -> HashSet<String> {
    let ignored = ignored_identities
        .iter()
        .flat_map(|identity| tokens(identity))
        .collect::<HashSet<_>>();
    tokens(query)
        .into_iter()
        .filter(|token| !ignored.contains(token))
        .collect()
}

fn tokens(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_lowercase();
            (token.chars().count() >= 2).then_some(token)
        })
        .collect()
}

fn bounded_label(label: String) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_relevance_promotes_specific_factual_pages_over_generic_results() {
        let links = vec![
            DiscoveredLink::search("https://example.test/".into(), "Example".into()),
            DiscoveredLink::search(
                "https://example.test/blog/company-news".into(),
                "Company news".into(),
            ),
            DiscoveredLink::search(
                "https://example.test/plans".into(),
                "Plans and pricing".into(),
            ),
        ];
        let ranked = rank("standard annual plan pricing", &[], links);
        assert_eq!(ranked[0].url, "https://example.test/plans");
    }

    #[test]
    fn exact_direct_sources_keep_priority_over_discovered_links() {
        let ranked = rank(
            "reference limits",
            &[],
            vec![
                DiscoveredLink::search(
                    "https://example.test/reference".into(),
                    "Reference limits".into(),
                ),
                DiscoveredLink::direct("https://example.test/requested".into()),
            ],
        );
        assert_eq!(ranked[0].url, "https://example.test/requested");
    }

    #[test]
    fn followups_require_query_overlap_in_label_or_path() {
        let relevant = relevant(
            "family plan pricing",
            &[],
            vec![
                DiscoveredLink::search(
                    "https://example.test/plans".into(),
                    "Plans and pricing".into(),
                ),
                DiscoveredLink::search(
                    "https://example.test/company".into(),
                    "Company history".into(),
                ),
            ],
        );
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].url, "https://example.test/plans");
    }

    #[test]
    fn site_identity_alone_does_not_make_a_navigation_link_relevant() {
        let relevant = relevant(
            "Example family pricing",
            &[],
            vec![
                DiscoveredLink::search("https://example.test/".into(), "Example".into()),
                DiscoveredLink::search(
                    "https://example.test/pricing".into(),
                    "Family pricing".into(),
                ),
            ],
        );
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].url, "https://example.test/pricing");
    }

    #[test]
    fn multiple_factual_matches_outrank_one_generic_label_match() {
        let relevant = relevant(
            "Example official family pricing",
            &[],
            vec![
                DiscoveredLink::search("https://example.test/".into(), "Example official".into()),
                DiscoveredLink::search(
                    "https://example.test/pricing".into(),
                    "Family pricing".into(),
                ),
            ],
        );
        assert_eq!(relevant[0].url, "https://example.test/pricing");
    }

    #[test]
    fn all_restricted_site_identities_are_removed_from_factual_ranking() {
        let ranked = rank(
            "Alpha Beta family annual pricing",
            &["alpha.test".into(), "beta.test".into()],
            vec![
                DiscoveredLink::search(
                    "https://alpha.test/alpha-vs-beta".into(),
                    "Alpha versus Beta".into(),
                ),
                DiscoveredLink::search(
                    "https://alpha.test/family-pricing".into(),
                    "Family annual pricing".into(),
                ),
            ],
        );
        assert_eq!(ranked[0].url, "https://alpha.test/family-pricing");
    }

    #[test]
    fn another_allowed_brand_name_cannot_make_a_followup_relevant() {
        let relevant = relevant(
            "Alpha Beta emergency access",
            &["alpha.test".into(), "beta.test".into()],
            vec![
                DiscoveredLink::search(
                    "https://alpha.test/compare-beta".into(),
                    "Why Alpha beats Beta".into(),
                ),
                DiscoveredLink::search(
                    "https://alpha.test/help/emergency-access".into(),
                    "Emergency access".into(),
                ),
            ],
        );
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].url, "https://alpha.test/help/emergency-access");
    }

    #[test]
    fn equal_relevance_prefers_the_shallower_factual_destination() {
        let relevant = relevant(
            "product pricing",
            &["example.test".into()],
            vec![
                DiscoveredLink::search(
                    "https://example.test/blog/archive/product".into(),
                    "Product".into(),
                ),
                DiscoveredLink::search("https://example.test/pricing".into(), "Pricing".into()),
            ],
        );
        assert_eq!(relevant[0].url, "https://example.test/pricing");
    }

    #[test]
    fn multiple_factual_matches_still_beat_a_shallower_single_match() {
        let relevant = relevant(
            "family emergency access",
            &[],
            vec![
                DiscoveredLink::search("https://example.test/family".into(), "Family".into()),
                DiscoveredLink::search(
                    "https://example.test/help/emergency-access".into(),
                    "Family emergency access".into(),
                ),
            ],
        );
        assert_eq!(
            relevant[0].url,
            "https://example.test/help/emergency-access"
        );
    }
}
