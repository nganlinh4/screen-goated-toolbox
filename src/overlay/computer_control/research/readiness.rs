use serde_json::Value;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::link_ranking::DiscoveredLink;
use super::search_provider::SearchProvider;

const CONTENT_READINESS_TIMEOUT: Duration = Duration::from_secs(8);
const CONTENT_READINESS_POLL: Duration = Duration::from_millis(100);
const CONTENT_STABLE_FOR: Duration = Duration::from_millis(250);
const MAX_SEARCH_CANDIDATES: usize = 40;
const MAX_SOURCE_LINKS: usize = 96;
const SEARCH_LINK_SCRIPT: &str = include_str!("search_links.js");
const SOURCE_LINK_SCRIPT: &str = include_str!("source_links.js");

pub(super) fn read_ready_page(tab_id: i64) -> anyhow::Result<Value> {
    let identity = super::super::browser::await_readable_document_on_tab(tab_id)?;
    let started = Instant::now();
    let mut stable = StableValue::default();
    loop {
        let capture =
            super::super::browser::capture_page_on_tab(tab_id).map_err(browser_capture_error)?;
        super::super::browser::validate_document_identity_on_tab(tab_id, &identity.document_id)?;
        if !same_canonical_url(capture.url(), &identity.url) {
            anyhow::bail!("browser document URL changed during source readiness");
        }
        let usable = !capture.title().trim().is_empty()
            && capture.text_char_count() >= super::result::MIN_SOURCE_CHARS
            && super::source_policy::canonical_url_key(capture.url()).is_some();
        if usable {
            let exact_fingerprint = capture.fingerprint();
            if stable.observe(exact_fingerprint, Instant::now(), CONTENT_STABLE_FOR) {
                poll_content_readiness(
                    started,
                    "stable readable source content",
                    "source changed before post-capture confirmation",
                )?;
                let post = super::super::browser::capture_page_on_tab(tab_id)
                    .map_err(browser_capture_error)?;
                super::super::browser::validate_document_identity_on_tab(
                    tab_id,
                    &identity.document_id,
                )?;
                if !same_canonical_url(post.url(), &identity.url) {
                    anyhow::bail!("browser document URL changed after source capture");
                }
                let post_usable = !post.title().trim().is_empty()
                    && post.text_char_count() >= super::result::MIN_SOURCE_CHARS
                    && super::source_policy::canonical_url_key(post.url()).is_some();
                if post_capture_matches(exact_fingerprint, post.fingerprint(), post_usable) {
                    let page = super::super::browser::publish_bounded_page_on_tab(capture, tab_id);
                    ensure_page_ok(&page)?;
                    super::super::browser::validate_document_identity_on_tab(
                        tab_id,
                        &identity.document_id,
                    )?;
                    if !same_canonical_url(super::result::page_url(&page), &identity.url) {
                        anyhow::bail!("browser document URL changed during source publication");
                    }
                    if super::result::source_is_usable(&page) {
                        return Ok(page);
                    }
                }
                if post_usable {
                    stable.observe(post.fingerprint(), Instant::now(), CONTENT_STABLE_FOR);
                } else {
                    stable.clear();
                }
                continue;
            }
        } else {
            stable.clear();
        }
        let last_error = if usable {
            "full source metadata and content hash did not remain stable"
        } else {
            "document has not exposed enough visible source text"
        };
        poll_content_readiness(started, "stable readable source content", last_error)?;
    }
}

pub(super) fn read_search_links_when_ready(
    tab_id: i64,
    provider: &SearchProvider,
) -> anyhow::Result<Vec<DiscoveredLink>> {
    let identity = super::super::browser::await_readable_document_on_tab(tab_id)?;
    let started = Instant::now();
    let mut stable = StableSearchLinks::default();
    loop {
        let (url, links, diagnostics) = search_link_sample(tab_id, provider)?;
        super::super::browser::validate_document_identity_on_tab(tab_id, &identity.document_id)?;
        if !same_canonical_url(&url, &identity.url) {
            anyhow::bail!("browser document URL changed during search readiness");
        }
        if let Some(stable_links) = stable.observe(links, Instant::now(), CONTENT_STABLE_FOR) {
            return Ok(stable_links);
        }
        let diagnostic = search_diagnostic_summary(&diagnostics);
        poll_content_readiness(started, "stable search result links", &diagnostic)?;
    }
}

pub(super) fn read_source_links(
    tab_id: i64,
    expected_url: &str,
) -> anyhow::Result<Vec<DiscoveredLink>> {
    let value = super::super::browser::eval_value_on_tab(SOURCE_LINK_SCRIPT, tab_id)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("source link extraction omitted url"))?;
    if !same_canonical_url(url, expected_url) {
        anyhow::bail!("browser document URL changed during source link extraction");
    }
    discovered_links(&value, MAX_SOURCE_LINKS, |url| Some(url.to_string()))
}

/// Search pages may append or reorder result links after first paint. A
/// non-empty baseline is stable when every baseline link remains present for
/// the readiness interval; unrelated additions do not invalidate it.
#[derive(Default)]
struct StableSearchLinks {
    baseline: Vec<DiscoveredLink>,
    baseline_set: HashSet<String>,
    since: Option<Instant>,
}

impl StableSearchLinks {
    fn observe(
        &mut self,
        links: Vec<DiscoveredLink>,
        now: Instant,
        stable_for: Duration,
    ) -> Option<Vec<DiscoveredLink>> {
        if links.is_empty() {
            self.clear();
            return None;
        }
        let current = links
            .iter()
            .map(|link| link.url.clone())
            .collect::<HashSet<_>>();
        if self.baseline.is_empty() || !current.is_superset(&self.baseline_set) {
            self.baseline = links;
            self.baseline_set = current;
            self.since = Some(now);
            return stable_for.is_zero().then(|| self.baseline.clone());
        }
        self.since
            .filter(|since| now.saturating_duration_since(*since) >= stable_for)
            .map(|_| self.baseline.clone())
    }

    fn clear(&mut self) {
        self.baseline.clear();
        self.baseline_set.clear();
        self.since = None;
    }
}

fn search_link_sample(
    tab_id: i64,
    provider: &SearchProvider,
) -> anyhow::Result<(String, Vec<DiscoveredLink>, Value)> {
    let value = super::super::browser::eval_value_on_tab(SEARCH_LINK_SCRIPT, tab_id)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("search link extraction omitted url"))?
        .to_string();
    let links = discovered_links(&value, MAX_SEARCH_CANDIDATES, |url| {
        provider.candidate_url(url)
    })?;
    let diagnostics = value.get("diagnostics").cloned().unwrap_or(Value::Null);
    Ok((url, links, diagnostics))
}

fn discovered_links(
    value: &Value,
    limit: usize,
    mut normalize_url: impl FnMut(&str) -> Option<String>,
) -> anyhow::Result<Vec<DiscoveredLink>> {
    Ok(value
        .get("links")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("search link extraction omitted links"))?
        .iter()
        .filter_map(|link| {
            let raw_url = link.get("url").and_then(Value::as_str)?;
            let url = normalize_url(raw_url)?;
            let label = link
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some(DiscoveredLink::search(url, label))
        })
        .take(limit)
        .collect())
}

fn search_diagnostic_summary(diagnostics: &Value) -> String {
    let count = |field| diagnostics.get(field).and_then(Value::as_u64).unwrap_or(0);
    format!(
        "no visible textual result links remained stable (scanned={}, hidden={}, chrome={}, headings={}, containers={}, unstructured={}, unlabeled={}, invalid_href={})",
        count("scanned"),
        count("hidden"),
        count("page_chrome"),
        count("heading_match"),
        count("container_match"),
        count("missing_structure"),
        count("missing_label"),
        count("invalid_href"),
    )
}

fn ensure_page_ok(page: &Value) -> anyhow::Result<()> {
    if page.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    anyhow::bail!(
        "{}: {}",
        page.get("code")
            .and_then(Value::as_str)
            .unwrap_or("ERR_BROWSER_READ_FAILED"),
        page.get("error")
            .and_then(Value::as_str)
            .unwrap_or("browser page read failed")
    )
}

fn browser_capture_error(value: Value) -> anyhow::Error {
    anyhow::anyhow!(
        "{}: {}",
        value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("ERR_BROWSER_READ_FAILED"),
        value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("browser page read failed")
    )
}

fn poll_content_readiness(
    started: Instant,
    expected: &str,
    last_error: &str,
) -> anyhow::Result<()> {
    if super::super::browser::action_cancelled() {
        anyhow::bail!("browser content readiness was cancelled");
    }
    if started.elapsed() >= CONTENT_READINESS_TIMEOUT {
        anyhow::bail!("browser did not expose {expected}: {last_error}");
    }
    if super::super::browser::pause_cancelled(CONTENT_READINESS_POLL) {
        anyhow::bail!("browser content readiness was cancelled");
    }
    Ok(())
}

fn same_canonical_url(left: &str, right: &str) -> bool {
    match (
        super::source_policy::canonical_url_key(left),
        super::source_policy::canonical_url_key(right),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => left == right,
    }
}

fn post_capture_matches(exact: [u8; 32], post: [u8; 32], post_usable: bool) -> bool {
    post_usable && exact == post
}

struct StableValue<T> {
    value: Option<T>,
    since: Option<Instant>,
}

impl<T> Default for StableValue<T> {
    fn default() -> Self {
        Self {
            value: None,
            since: None,
        }
    }
}

impl<T: PartialEq> StableValue<T> {
    fn observe(&mut self, value: T, now: Instant, stable_for: Duration) -> bool {
        if self.value.as_ref() != Some(&value) {
            self.value = Some(value);
            self.since = Some(now);
            return stable_for.is_zero();
        }
        self.since
            .is_some_and(|since| now.saturating_duration_since(since) >= stable_for)
    }

    fn clear(&mut self) {
        self.value = None;
        self.since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(url: &str) -> DiscoveredLink {
        DiscoveredLink::search(url.to_string(), url.to_string())
    }

    #[test]
    fn publication_requires_a_stable_interval_and_matching_post_capture() {
        let start = Instant::now();
        let mut stable = StableValue::default();
        assert!(!stable.observe(1, start, Duration::from_millis(250)));
        assert!(!stable.observe(
            1,
            start + Duration::from_millis(249),
            Duration::from_millis(250)
        ));
        assert!(stable.observe(
            1,
            start + Duration::from_millis(250),
            Duration::from_millis(250)
        ));
        assert!(!stable.observe(
            2,
            start + Duration::from_millis(400),
            Duration::from_millis(250)
        ));
        assert!(!stable.observe(
            2,
            start + Duration::from_millis(649),
            Duration::from_millis(250)
        ));
        assert!(stable.observe(
            2,
            start + Duration::from_millis(650),
            Duration::from_millis(250)
        ));
        assert!(!post_capture_matches([1; 32], [2; 32], true));
        assert!(!post_capture_matches([1; 32], [1; 32], false));
        assert!(post_capture_matches([1; 32], [1; 32], true));
    }

    #[test]
    fn search_discovery_requires_structural_rendered_result_links() {
        assert!(SEARCH_LINK_SCRIPT.contains("if (!visible(anchor))"));
        assert!(SEARCH_LINK_SCRIPT.contains("if (isPageChrome(anchor))"));
        assert!(SEARCH_LINK_SCRIPT.contains("const headingMatch = Boolean(resultHeading(anchor))"));
        assert!(SEARCH_LINK_SCRIPT.contains("main,[role='main']"));
        assert!(SEARCH_LINK_SCRIPT.contains("if (!headingMatch && !containerMatch)"));
        assert!(SEARCH_LINK_SCRIPT.contains("header,nav,footer,aside"));
        assert!(SEARCH_LINK_SCRIPT.contains("anchor.getAttribute?.(\"aria-label\")"));
        assert!(SEARCH_LINK_SCRIPT.contains(".find((value) => value.length > 0)"));
        assert!(SEARCH_LINK_SCRIPT.contains("diagnostics.missing_label++"));
        assert!(SEARCH_LINK_SCRIPT.contains("links.push({url: href, label:"));
        assert!(SEARCH_LINK_SCRIPT.contains("return {url: location.href, links, diagnostics}"));
    }

    #[test]
    fn source_discovery_keeps_visible_navigation_labels_and_full_targets() {
        assert!(SOURCE_LINK_SCRIPT.contains("anchor.getAttribute?.(\"aria-label\")"));
        assert!(SOURCE_LINK_SCRIPT.contains("target.hash = \"\""));
        assert!(SOURCE_LINK_SCRIPT.contains("links.push({url, label:"));
        assert!(!SOURCE_LINK_SCRIPT.contains("isPageChrome"));
    }

    #[test]
    fn empty_search_diagnostics_are_bounded_and_actionable() {
        let summary = search_diagnostic_summary(&serde_json::json!({
            "scanned": 12,
            "container_match": 3,
            "missing_label": 2,
        }));
        assert!(summary.contains("scanned=12"));
        assert!(summary.contains("containers=3"));
        assert!(summary.contains("unlabeled=2"));
    }

    #[test]
    fn search_readiness_tolerates_reordering_and_appended_links() {
        let start = Instant::now();
        let mut stable = StableSearchLinks::default();
        assert!(
            stable
                .observe(
                    vec![link("https://one.test/"), link("https://two.test/")],
                    start,
                    Duration::from_millis(250),
                )
                .is_none()
        );
        assert!(
            stable
                .observe(
                    vec![
                        link("https://two.test/"),
                        link("https://one.test/"),
                        link("https://three.test/"),
                    ],
                    start + Duration::from_millis(249),
                    Duration::from_millis(250),
                )
                .is_none()
        );
        assert_eq!(
            stable.observe(
                vec![
                    link("https://three.test/"),
                    link("https://one.test/"),
                    link("https://two.test/"),
                ],
                start + Duration::from_millis(250),
                Duration::from_millis(250),
            ),
            Some(vec![link("https://one.test/"), link("https://two.test/")])
        );
    }

    #[test]
    fn disappearing_search_link_restarts_readiness_interval() {
        let start = Instant::now();
        let mut stable = StableSearchLinks::default();
        assert!(
            stable
                .observe(
                    vec![link("https://one.test/"), link("https://two.test/")],
                    start,
                    Duration::from_millis(250),
                )
                .is_none()
        );
        assert!(
            stable
                .observe(
                    vec![link("https://one.test/")],
                    start + Duration::from_millis(250),
                    Duration::from_millis(250),
                )
                .is_none()
        );
        assert_eq!(
            stable.observe(
                vec![link("https://one.test/")],
                start + Duration::from_millis(500),
                Duration::from_millis(250),
            ),
            Some(vec![link("https://one.test/")])
        );
    }
}
