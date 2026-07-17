//! Bounded same-site follow-up queueing for already captured research pages.

use std::collections::{HashSet, VecDeque};

use super::link_ranking::{self, DiscoveredLink};
use super::source_policy::{self, SourcePolicy};

const MAX_FOLLOW_UPS_PER_SOURCE: usize = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingLink {
    url: String,
    may_expand: bool,
}

impl PendingLink {
    pub(super) fn initial(url: String) -> Self {
        Self {
            url,
            may_expand: true,
        }
    }

    fn follow_up(url: String) -> Self {
        Self {
            url,
            may_expand: false,
        }
    }

    pub(super) fn url(&self) -> &str {
        &self.url
    }

    pub(super) fn may_expand(&self) -> bool {
        self.may_expand
    }
}

pub(super) struct QueueState<'a> {
    pub(super) pending: &'a mut VecDeque<PendingLink>,
    pub(super) queued_urls: &'a mut HashSet<String>,
    pub(super) candidate_count: &'a mut usize,
    pub(super) max_candidates: usize,
}

pub(super) fn enqueue(
    query: &str,
    current_url: &str,
    policy: &SourcePolicy,
    links: Vec<DiscoveredLink>,
    state: QueueState<'_>,
) -> usize {
    let QueueState {
        pending,
        queued_urls,
        candidate_count,
        max_candidates,
    } = state;
    let current_key = source_policy::canonical_url_key(current_url);
    let mut accepted = Vec::new();
    for link in link_ranking::relevant(query, policy.relevance_identity_terms(), links) {
        if *candidate_count >= max_candidates
            || accepted.len() >= MAX_FOLLOW_UPS_PER_SOURCE
            || !policy.accepts_redirect(current_url, &link.url)
        {
            continue;
        }
        let Some(key) = source_policy::canonical_url_key(&link.url) else {
            continue;
        };
        if current_key.as_ref() == Some(&key) {
            continue;
        }
        if queued_urls.insert(key) {
            accepted.push(link.url);
            *candidate_count = (*candidate_count).saturating_add(1);
        }
    }
    let accepted_count = accepted.len();
    for url in accepted.into_iter().rev() {
        pending.push_front(PendingLink::follow_up(url));
    }
    accepted_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relevant_same_site_followup_is_prioritized_without_crossing_scope() {
        let policy = SourcePolicy::parse(
            &json!({
                "source_policy": "domain_restricted",
                "allowed_domains": ["example.test"],
            }),
            3,
        )
        .unwrap();
        let current = "https://example.test/";
        let mut pending = VecDeque::new();
        let mut queued = HashSet::from([source_policy::canonical_url_key(current).unwrap()]);
        let mut candidate_count = 1;
        let accepted_count = enqueue(
            "standard annual plan pricing",
            current,
            &policy,
            vec![
                DiscoveredLink::search(
                    "https://example.test/".into(),
                    "Standard annual plan pricing".into(),
                ),
                DiscoveredLink::search(
                    "https://example.test/plans".into(),
                    "Plans and pricing".into(),
                ),
                DiscoveredLink::search(
                    "https://outside.test/plans".into(),
                    "Plans and pricing".into(),
                ),
                DiscoveredLink::search(
                    "https://example.test/company".into(),
                    "Company history".into(),
                ),
            ],
            QueueState {
                pending: &mut pending,
                queued_urls: &mut queued,
                candidate_count: &mut candidate_count,
                max_candidates: 40,
            },
        );
        assert_eq!(accepted_count, 1);
        assert_eq!(
            pending.iter().map(|link| link.url()).collect::<Vec<_>>(),
            ["https://example.test/plans"]
        );
        assert!(!pending[0].may_expand());
        assert_eq!(candidate_count, 2);
    }
}
