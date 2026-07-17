//! Browser-backed research helper.
//!
//! This tool gives the harness one compact, source-aware path for web
//! verification instead of relying on the live model to sequence search, source
//! selection, page reads, and provenance itself.

use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;

mod diagnostics;
mod direct_sources;
mod followups;
mod link_ranking;
mod readiness;
mod result;
mod search_provider;
mod source_policy;
mod temporary_tab;
use diagnostics::{FailureClass, SourceDiagnostics};
use link_ranking::DiscoveredLink;
use result::{
    ResearchSource, page_url, research_result, source_content_hash, source_from_page,
    source_is_usable,
};
use source_policy::SourcePolicy;
use temporary_tab::TemporaryTab;

const MAX_SEARCH_CANDIDATES: usize = 40;
const MAX_RESEARCH_QUERY_CHARS: usize = 512;
const MAX_RESEARCH_PURPOSE_CHARS: usize = 512;
const MAX_EVIDENCE_QUERY_CHARS: usize = 1024;
const RESEARCH_REQUEST_DEADLINE: Duration = Duration::from_secs(45);
const MIN_SOURCES_BEFORE_FAILURE_CUTOFF: usize = 2;
const MAX_CONSECUTIVE_SOURCE_FAILURES_WITH_EVIDENCE: usize = 3;

pub(super) fn research_web(args: &Value) -> Value {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_QUERY_REQUIRED",
            "error": "research_web needs a non-empty query",
            "read_only": true,
            "effect_may_have_occurred": false,
            "executed": false,
        });
    }
    if query.chars().count() > MAX_RESEARCH_QUERY_CHARS {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_QUERY_TOO_LONG",
            "error": format!("research_web query must be at most {MAX_RESEARCH_QUERY_CHARS} characters"),
            "read_only": true,
            "effect_may_have_occurred": false,
            "executed": false,
        });
    }
    let purpose = args
        .get("purpose")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if purpose.is_empty() {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_PURPOSE_REQUIRED",
            "error": "research_web needs a public evidence scope with the exact subjects, fields, units or basis, and qualifiers to resolve",
            "read_only": true,
            "effect_may_have_occurred": false,
            "executed": false,
        });
    }
    if purpose.chars().count() > MAX_RESEARCH_PURPOSE_CHARS {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_PURPOSE_TOO_LONG",
            "error": format!("research_web purpose must be at most {MAX_RESEARCH_PURPOSE_CHARS} characters"),
            "read_only": true,
            "effect_may_have_occurred": false,
            "executed": false,
        });
    }
    let max_sources = args
        .get("max_sources")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 5) as usize;
    let policy = match SourcePolicy::parse(args, max_sources) {
        Ok(policy) => policy,
        Err(error) => return error,
    };
    let direct_links = match direct_sources::parse(args, &policy, max_sources) {
        Ok(links) => links,
        Err(error) => return error,
    };

    super::telemetry::event(
        "research_start",
        "research",
        super::telemetry::Privacy::Safe,
        json!({
            "query_char_count": query.chars().count(),
            "query_byte_count": query.len(),
            "source_policy": policy.name(),
            "max_sources": max_sources,
            "direct_source_count": direct_links.len(),
        }),
    );

    if !super::browser::is_connected() {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_BROWSER_NOT_CONNECTED",
            "error": "deep browser control is not connected",
            "read_only": true,
            "effect_may_have_occurred": false,
            "executed": false,
            "instruction": "Call browser_status/browser_setup, or answer that web verification cannot run until browser control is connected.",
        });
    }

    let _request_deadline = super::browser::enter_request_deadline(RESEARCH_REQUEST_DEADLINE);

    let discovery_query = discovery_query(query);
    let search_query = policy.search_query(&discovery_query);
    let evidence_query = evidence_query(query, purpose);
    let mut diagnostics = SourceDiagnostics::default();
    let mut discovered_links = direct_links
        .into_iter()
        .map(DiscoveredLink::direct)
        .collect::<Vec<_>>();
    match discover_links(&search_query, &mut diagnostics) {
        Ok(links) => discovered_links.extend(links),
        Err(error) => {
            if discovered_links.is_empty() {
                diagnostics.failed(
                    FailureClass::Discovery,
                    "search_page_unavailable",
                    format!("search page unavailable: {error}"),
                );
            } else {
                super::telemetry::event(
                    "research_supplemental_discovery_unavailable",
                    "research",
                    super::telemetry::Privacy::Safe,
                    json!({"direct_source_count": discovered_links.len()}),
                );
            }
        }
    }
    for recovery_query in policy.missing_domain_search_queries(
        &discovery_query,
        discovered_links.iter().map(|link| link.url.as_str()),
    ) {
        match discover_links(&recovery_query, &mut diagnostics) {
            Ok(links) => discovered_links.extend(links),
            Err(error) => diagnostics.failed(
                FailureClass::Discovery,
                "domain_search_page_unavailable",
                format!("domain-specific search page unavailable: {error}"),
            ),
        }
    }
    let ranked_links = link_ranking::rank(
        &evidence_query,
        policy.relevance_identity_terms(),
        discovered_links,
    );
    let links = policy.select_candidates(
        ranked_links.into_iter().map(|link| link.url).collect(),
        (max_sources * 4).min(MAX_SEARCH_CANDIDATES),
    );
    diagnostics.initial_candidate_count = links.len();
    super::telemetry::event(
        "research_initial_candidates",
        "research",
        super::telemetry::Privacy::Sensitive,
        json!({
            "source_urls": links.iter().filter_map(|url| {
                source_policy::safe_url_for_output(url).map(|(safe, _)| safe)
            }).collect::<Vec<_>>(),
        }),
    );
    let mut candidate_count = links.len();
    let mut queued_urls = links
        .iter()
        .filter_map(|url| source_policy::canonical_url_key(url))
        .collect::<HashSet<_>>();
    let mut pending_links = links
        .into_iter()
        .map(followups::PendingLink::initial)
        .collect::<VecDeque<_>>();
    let mut sources = Vec::new();
    let mut deferred_sources = Vec::new();
    let mut seen_final_urls = HashSet::new();
    let mut seen_content_hashes = HashSet::new();
    let mut consecutive_source_failures = 0usize;
    while let Some(pending_link) = pending_links.pop_front() {
        let link = pending_link.url();
        let current_coverage = policy.coverage(
            sources
                .iter()
                .map(|source: &ResearchSource| source.url.as_str()),
        );
        let coverage_satisfied = !current_coverage.assessed || current_coverage.missing.is_empty();
        if sources.len() >= max_sources {
            break;
        }
        let tab = match TemporaryTab::open(link, &mut diagnostics, FailureClass::Source) {
            Ok(tab) => tab,
            Err(error) => {
                diagnostics.failed(
                    FailureClass::Source,
                    "source_tab_open_failed",
                    format!("source tab open failed: {error}"),
                );
                if source_failure_cutoff(
                    sources.len() + deferred_sources.len(),
                    coverage_satisfied,
                    &mut consecutive_source_failures,
                    &mut diagnostics,
                ) {
                    break;
                }
                continue;
            }
        };
        let page = readiness::read_ready_page(tab.id());
        let follow_up_links = if pending_link.may_expand() {
            if let Ok(page) = page.as_ref() {
                diagnostics.source_link_page_count =
                    diagnostics.source_link_page_count.saturating_add(1);
                readiness::read_source_links(tab.id(), page_url(page)).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        tab.close(&mut diagnostics);
        let page = match page {
            Ok(page) => page,
            Err(error) => {
                diagnostics.failed(
                    FailureClass::Source,
                    "source_page_unavailable",
                    format!("source page unavailable: {error}"),
                );
                if source_failure_cutoff(
                    sources.len() + deferred_sources.len(),
                    coverage_satisfied,
                    &mut consecutive_source_failures,
                    &mut diagnostics,
                ) {
                    break;
                }
                continue;
            }
        };
        let final_url = page_url(&page);
        if !policy.accepts_redirect(link, final_url) {
            diagnostics.rejected_domain_count += 1;
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        }
        diagnostics.follow_up_candidate_count = diagnostics
            .follow_up_candidate_count
            .saturating_add(followups::enqueue(
                &evidence_query,
                final_url,
                &policy,
                follow_up_links,
                followups::QueueState {
                    pending: &mut pending_links,
                    queued_urls: &mut queued_urls,
                    candidate_count: &mut candidate_count,
                    max_candidates: MAX_SEARCH_CANDIDATES,
                },
            ));
        if !source_is_usable(&page) {
            diagnostics.empty_source_count += 1;
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        }
        let Some(canonical_url) = source_policy::canonical_url_key(final_url) else {
            diagnostics.failed(
                FailureClass::Source,
                "source_url_not_canonical",
                "source returned no canonical HTTP URL",
            );
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        };
        if !seen_final_urls.insert(canonical_url.clone()) {
            diagnostics.duplicate_source_count += 1;
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        }
        let content_hash = source_content_hash(&page);
        if !seen_content_hashes.insert(content_hash) {
            diagnostics.duplicate_source_count += 1;
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        }
        let Some((safe_url, query_omitted)) = source_policy::safe_url_for_output(final_url) else {
            diagnostics.failed(
                FailureClass::Source,
                "source_url_not_exposable",
                "source URL was not safe to expose as a citation",
            );
            if source_failure_cutoff(
                sources.len() + deferred_sources.len(),
                coverage_satisfied,
                &mut consecutive_source_failures,
                &mut diagnostics,
            ) {
                break;
            }
            continue;
        };
        let source = source_from_page(&page, safe_url, query_omitted);
        match source_slot(
            sources.len(),
            max_sources,
            policy.requested_domain(final_url).as_deref(),
            &current_coverage,
        ) {
            SourceSlot::Accept => {
                sources.push(source);
                consecutive_source_failures = 0;
            }
            SourceSlot::Defer => {
                deferred_sources.push(source);
                consecutive_source_failures = 0;
            }
            SourceSlot::Full => break,
        }
    }
    sources.extend(
        deferred_sources
            .into_iter()
            .take(max_sources.saturating_sub(sources.len())),
    );
    debug_assert_eq!(
        candidate_count,
        diagnostics
            .initial_candidate_count
            .saturating_add(diagnostics.follow_up_candidate_count)
    );
    debug_assert!(diagnostics.source_link_page_count <= diagnostics.initial_candidate_count);

    let coverage = policy.coverage(sources.iter().map(|source| source.url.as_str()));
    research_result(
        query,
        &evidence_query,
        policy.name(),
        &sources,
        coverage,
        candidate_count,
        &diagnostics,
    )
}

fn source_failure_cutoff(
    usable_source_count: usize,
    coverage_satisfied: bool,
    consecutive_failures: &mut usize,
    diagnostics: &mut SourceDiagnostics,
) -> bool {
    *consecutive_failures = consecutive_failures.saturating_add(1);
    let reached = coverage_satisfied
        && usable_source_count >= MIN_SOURCES_BEFORE_FAILURE_CUTOFF
        && *consecutive_failures >= MAX_CONSECUTIVE_SOURCE_FAILURES_WITH_EVIDENCE;
    if reached {
        diagnostics.source_failure_cutoff_reached = true;
        diagnostics.consecutive_source_failures_at_cutoff = *consecutive_failures;
    }
    reached
}

fn discovery_query(query: &str) -> String {
    query.chars().take(MAX_RESEARCH_QUERY_CHARS).collect()
}

fn evidence_query(query: &str, purpose: &str) -> String {
    if purpose.is_empty() {
        return query.to_string();
    }
    format!("{query} {purpose}")
        .chars()
        .take(MAX_EVIDENCE_QUERY_CHARS)
        .collect()
}

fn discover_links(
    query: &str,
    diagnostics: &mut SourceDiagnostics,
) -> anyhow::Result<Vec<DiscoveredLink>> {
    let mut failures = Vec::new();
    for (index, provider) in search_provider::providers().iter().enumerate() {
        match discover_links_with(query, provider, diagnostics) {
            Ok(links) => {
                if index > 0 {
                    super::telemetry::event(
                        "research_search_provider_recovered",
                        "research",
                        super::telemetry::Privacy::Safe,
                        json!({"provider": provider.name(), "fallback_index": index}),
                    );
                }
                return Ok(links);
            }
            Err(error) => {
                super::telemetry::event(
                    "research_search_provider_unavailable",
                    "research",
                    super::telemetry::Privacy::Safe,
                    json!({
                        "provider": provider.name(),
                        "fallback_index": index,
                        "error": super::telemetry::value_metadata(&json!(error.to_string())),
                    }),
                );
                failures.push(format!("{}: {error}", provider.name()));
            }
        }
    }
    anyhow::bail!(
        "all bounded search providers failed: {}",
        failures.join(" | ")
    )
}

fn discover_links_with(
    query: &str,
    provider: &search_provider::SearchProvider,
    diagnostics: &mut SourceDiagnostics,
) -> anyhow::Result<Vec<DiscoveredLink>> {
    let search_url = provider
        .search_url(query)
        .map_err(|error| anyhow::anyhow!("search provider URL construction failed: {error}"))?;
    let search_tab = TemporaryTab::open(&search_url, diagnostics, FailureClass::Discovery)
        .map_err(|error| anyhow::anyhow!("search tab open failed: {error}"))?;
    let links = readiness::read_search_links_when_ready(search_tab.id(), provider);
    search_tab.close(diagnostics);
    links
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceSlot {
    Accept,
    Defer,
    Full,
}

fn source_slot(
    accepted: usize,
    max_sources: usize,
    requested_domain: Option<&str>,
    coverage: &source_policy::Coverage,
) -> SourceSlot {
    if accepted >= max_sources {
        return SourceSlot::Full;
    }
    let already_covered = requested_domain
        .is_some_and(|domain| coverage.covered.iter().any(|covered| covered == domain));
    if already_covered && accepted + coverage.missing.len() >= max_sources {
        SourceSlot::Defer
    } else {
        SourceSlot::Accept
    }
}

#[cfg(test)]
#[path = "research/request_tests.rs"]
mod tests;
