//! Bounded readiness for reading a newly opened exact browser tab.

use anyhow::{Result, anyhow};
use serde_json::Value;
use std::time::{Duration, Instant};

use super::frame_identity::StableDocumentIdentity;

const READINESS_TIMEOUT: Duration = Duration::from_secs(8);
const READINESS_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq, Eq)]
struct DocumentReadiness {
    ready_state: String,
    has_body: bool,
    url: String,
}

pub(in crate::overlay::computer_control) fn await_readable_document_on_tab(
    tab_id: i64,
) -> Result<StableDocumentIdentity> {
    let started = Instant::now();
    loop {
        let last_error = match super::frame_identity::acquire_stable_document_identity_on_tab(
            tab_id, None, true,
        ) {
            Ok(identity) => match read_readiness(tab_id) {
                Ok(readiness) if is_ready_for(&readiness, &identity.url) => {
                    match super::frame_identity::validate_document_identity_on_tab(
                        tab_id,
                        &identity.document_id,
                    ) {
                        Ok(()) => return Ok(identity),
                        Err(error) => error.to_string(),
                    }
                }
                Ok(readiness) => format!(
                    "document state={} body={} committed_url_match={}",
                    readiness.ready_state,
                    readiness.has_body,
                    urls_equivalent(&readiness.url, &identity.url)
                ),
                Err(error) => error.to_string(),
            },
            Err(error) => error.to_string(),
        };
        if super::readiness::action_cancelled() {
            anyhow::bail!("browser document readiness was cancelled");
        }
        if started.elapsed() >= READINESS_TIMEOUT {
            anyhow::bail!(
                "browser tab {tab_id} did not expose a stable readable document: {}",
                last_error
            );
        }
        if super::readiness::pause_cancelled(READINESS_POLL) {
            anyhow::bail!("browser document readiness was cancelled");
        }
    }
}

fn read_readiness(tab_id: i64) -> Result<DocumentReadiness> {
    let value = super::eval_value_on_tab(
        "({readyState: document.readyState, hasBody: !!document.body, url: location.href})",
        tab_id,
    )?;
    readiness_from_value(&value)
}

fn readiness_from_value(value: &Value) -> Result<DocumentReadiness> {
    let string = |name| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("browser document readiness omitted {name}"))
    };
    Ok(DocumentReadiness {
        ready_state: string("readyState")?,
        has_body: value
            .get("hasBody")
            .and_then(Value::as_bool)
            .ok_or_else(|| anyhow!("browser document readiness omitted hasBody"))?,
        url: string("url")?,
    })
}

fn is_ready_for(readiness: &DocumentReadiness, committed_url: &str) -> bool {
    readiness.has_body
        && matches!(readiness.ready_state.as_str(), "interactive" | "complete")
        && urls_equivalent(&readiness.url, committed_url)
}

fn urls_equivalent(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    match (url::Url::parse(left), url::Url::parse(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_settled_body_on_the_committed_url_is_readable() {
        let complete = readiness_from_value(&json!({
            "readyState": "complete",
            "hasBody": true,
            "url": "https://example.test/"
        }))
        .unwrap();
        assert!(is_ready_for(&complete, "https://example.test"));

        let loading = DocumentReadiness {
            ready_state: "loading".into(),
            ..complete
        };
        assert!(!is_ready_for(&loading, "https://example.test/"));

        let unrelated = DocumentReadiness {
            ready_state: "interactive".into(),
            has_body: true,
            url: "https://other.test/".into(),
        };
        assert!(!is_ready_for(&unrelated, "https://example.test/"));
    }

    #[test]
    fn malformed_readiness_is_not_silently_accepted() {
        assert!(readiness_from_value(&json!({"readyState": "complete"})).is_err());
    }
}
