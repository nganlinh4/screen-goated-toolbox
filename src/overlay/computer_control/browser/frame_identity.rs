//! Exact browser document identities used to bind model-visible frames to actions.

use anyhow::{Result, anyhow};
use serde_json::Value;
use std::time::{Duration, Instant};

use super::super::controller::world::BrowserWindowIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
struct MainDocumentState {
    frame_id: String,
    loader_id: String,
    url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct StableDocumentIdentity {
    pub document_id: String,
    pub loader_id: String,
    pub url: String,
}

fn document_id(value: Value) -> Result<String> {
    value
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("browser document identity is unavailable"))
}

pub(in crate::overlay::computer_control) fn active_document_identity()
-> Result<(i64, String, BrowserWindowIdentity)> {
    let (tab_id, window) = super::surface_binding::active()?;
    let value = super::controller_io::eval_value_in_active_tab(
        super::controller_io::DOCUMENT_ID_JS,
        tab_id,
        None,
    )?;
    let document_id = document_id(value)?;
    super::surface_binding::validate(tab_id, &window)?;
    Ok((tab_id, document_id, window))
}

pub(in crate::overlay::computer_control) fn document_identity_on_tab(
    tab_id: i64,
) -> Result<String> {
    document_id(super::eval_value_on_tab(
        super::controller_io::DOCUMENT_ID_JS,
        tab_id,
    )?)
}

/// Resolve the top document after a tab activation or verified navigation. Two
/// consecutive loader/document samples must match; navigation can otherwise
/// replace an initial execution context immediately after we stamp its ID.
pub(in crate::overlay::computer_control) fn acquire_stable_document_identity_on_tab(
    tab_id: i64,
    expected_loader_id: Option<&str>,
    require_settled_tab: bool,
) -> Result<StableDocumentIdentity> {
    if tab_id <= 0 {
        anyhow::bail!("browser document binding needs a positive tab id");
    }
    let started = Instant::now();
    let timeout = Duration::from_secs(2);
    let mut attempts = 0_u32;
    let mut previous = None;
    loop {
        attempts = attempts.saturating_add(1);
        match read_binding_candidate(tab_id, expected_loader_id, require_settled_tab) {
            Ok(candidate) if candidate_is_stable(previous.as_ref(), &candidate) => {
                return Ok(candidate);
            }
            Ok(candidate) => previous = Some(candidate),
            Err(error) => {
                previous = None;
                if super::readiness::action_cancelled() || started.elapsed() >= timeout {
                    anyhow::bail!(
                        "browser document identity remained unstable on tab {tab_id} after {attempts} attempts: {error}"
                    );
                }
            }
        }
        if super::readiness::action_cancelled() {
            anyhow::bail!("browser document identity acquisition was cancelled");
        }
        if started.elapsed() >= timeout {
            anyhow::bail!(
                "browser document identity did not remain stable on tab {tab_id} after {attempts} attempts"
            );
        }
        std::thread::sleep(Duration::from_millis(75));
    }
}

fn candidate_is_stable(
    previous: Option<&StableDocumentIdentity>,
    current: &StableDocumentIdentity,
) -> bool {
    previous == Some(current)
}

fn read_binding_candidate(
    tab_id: i64,
    expected_loader_id: Option<&str>,
    require_settled_tab: bool,
) -> Result<StableDocumentIdentity> {
    if require_settled_tab && tab_has_pending_navigation(tab_id)? {
        anyhow::bail!("browser tab still has a pending navigation");
    }
    let before = main_document_state(tab_id)?;
    let document_id = document_identity_on_tab(tab_id)?;
    let after = main_document_state(tab_id)?;
    if require_settled_tab && tab_has_pending_navigation(tab_id)? {
        anyhow::bail!("browser tab began another navigation while acquiring its identity");
    }
    validate_binding_candidate(
        &before,
        &after,
        document_id,
        expected_loader_id,
        require_settled_tab,
    )
}

fn tab_has_pending_navigation(tab_id: i64) -> Result<bool> {
    let tabs = super::bridge::rpc("tabs", serde_json::json!({"action": "list"}))?;
    let tab = tabs
        .as_array()
        .and_then(|tabs| {
            tabs.iter()
                .find(|tab| tab.get("id").and_then(Value::as_i64) == Some(tab_id))
        })
        .ok_or_else(|| anyhow!("browser tab {tab_id} is no longer available"))?;
    Ok(tab
        .get("pendingUrl")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.is_empty()))
}

fn main_document_state(tab_id: i64) -> Result<MainDocumentState> {
    let tree = super::bridge::cdp_on_tab("Page.getFrameTree", serde_json::json!({}), tab_id)?;
    main_document_state_from_tree(&tree)
}

fn main_document_state_from_tree(tree: &Value) -> Result<MainDocumentState> {
    let frame = tree
        .pointer("/frameTree/frame")
        .ok_or_else(|| anyhow!("browser did not return a main-frame state"))?;
    let required = |field: &str| {
        frame
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("browser main frame omitted {field}"))
    };
    Ok(MainDocumentState {
        frame_id: required("id")?,
        loader_id: required("loaderId")?,
        url: frame
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn validate_binding_candidate(
    before: &MainDocumentState,
    after: &MainDocumentState,
    document_id: String,
    expected_loader_id: Option<&str>,
    reject_transient_blank: bool,
) -> Result<StableDocumentIdentity> {
    if before != after {
        anyhow::bail!("browser main document changed while acquiring its identity");
    }
    if expected_loader_id.is_some_and(|expected| before.loader_id != expected) {
        anyhow::bail!("browser main document does not match the navigation dispatch loader");
    }
    if reject_transient_blank && !super::tab_lifecycle::bindable_document_url(&before.url) {
        anyhow::bail!("browser main document still has a transient or malformed URL");
    }
    Ok(StableDocumentIdentity {
        document_id,
        loader_id: before.loader_id.clone(),
        url: before.url.clone(),
    })
}

pub(in crate::overlay::computer_control) fn validate_active_document_identity(
    tab_id: i64,
    expected_document_id: &str,
    expected_window: &BrowserWindowIdentity,
) -> Result<()> {
    let actual = active_document_identity()?;
    if actual.0 == tab_id && actual.1 == expected_document_id && actual.2 == *expected_window {
        Ok(())
    } else {
        anyhow::bail!(
            "active browser surface changed; expected tab/document/window {tab_id}/{expected_document_id}/{expected_window:?}, got {}/{}/{:?}",
            actual.0,
            actual.1,
            actual.2
        )
    }
}

pub(in crate::overlay::computer_control) fn validate_document_identity_on_tab(
    tab_id: i64,
    expected_document_id: &str,
) -> Result<()> {
    let actual = document_identity_on_tab(tab_id)?;
    if actual == expected_document_id {
        Ok(())
    } else {
        anyhow::bail!(
            "browser document changed; expected {expected_document_id}, got {actual} on tab {tab_id}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_identity_requires_a_nonempty_string() {
        assert_eq!(document_id(Value::String("doc-1".into())).unwrap(), "doc-1");
        assert!(document_id(Value::String(String::new())).is_err());
        assert!(document_id(Value::Null).is_err());
        assert!(
            acquire_stable_document_identity_on_tab(0, None, false)
                .unwrap_err()
                .to_string()
                .contains("positive tab id")
        );
    }

    fn state(url: &str, loader_id: &str) -> MainDocumentState {
        MainDocumentState {
            frame_id: "main-frame".into(),
            loader_id: loader_id.into(),
            url: url.into(),
        }
    }

    #[test]
    fn document_binding_rejects_loader_swap_and_unrelated_loader() {
        let before = state("https://requested.invalid/", "loader-requested");
        let swapped = state("https://unrelated.invalid/", "loader-unrelated");
        assert!(
            validate_binding_candidate(
                &before,
                &swapped,
                "doc-before".into(),
                Some("loader-requested"),
                false,
            )
            .is_err()
        );
        assert!(
            validate_binding_candidate(
                &swapped,
                &swapped,
                "doc-unrelated".into(),
                Some("loader-requested"),
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn document_binding_requires_stable_nonblank_pending_document() {
        let blank = state("about:blank", "loader-blank");
        assert!(
            validate_binding_candidate(&blank, &blank, "doc-blank".into(), None, true,).is_err()
        );
        let synthetic = state(":", "loader-synthetic");
        assert!(
            validate_binding_candidate(&synthetic, &synthetic, "doc-synthetic".into(), None, true,)
                .is_err()
        );

        let committed = state("https://requested.invalid/", "loader-requested");
        let bound = validate_binding_candidate(
            &committed,
            &committed,
            "doc-requested".into(),
            Some("loader-requested"),
            true,
        )
        .unwrap();
        assert_eq!(bound.document_id, "doc-requested");
        assert_eq!(bound.loader_id, "loader-requested");

        let swapped = StableDocumentIdentity {
            document_id: "doc-next".into(),
            loader_id: "loader-next".into(),
            url: "https://next.invalid/".into(),
        };
        assert!(candidate_is_stable(Some(&bound), &bound));
        assert!(!candidate_is_stable(Some(&bound), &swapped));
    }
}
