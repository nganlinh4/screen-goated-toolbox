use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MainFrameState {
    pub(super) url: String,
    pub(super) unreachable_url: Option<String>,
    pub(super) loader_id: Option<String>,
}

impl MainFrameState {
    pub(super) fn committed_url(&self) -> &str {
        self.unreachable_url.as_deref().unwrap_or(&self.url)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NavigationOutcome {
    Direct,
    Redirect,
    AlreadyAtDestination,
    LoadFailed,
}

impl NavigationOutcome {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Redirect => "redirect",
            Self::AlreadyAtDestination => "already_at_destination",
            Self::LoadFailed => "load_failed",
        }
    }
}

pub(super) fn main_frame_from_tree(tree: &Value) -> anyhow::Result<MainFrameState> {
    let frame = tree
        .pointer("/frameTree/frame")
        .ok_or_else(|| anyhow::anyhow!("browser did not return a main-frame state"))?;
    let url = frame
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("browser main frame omitted its URL"))?
        .to_string();
    Ok(MainFrameState {
        url,
        unreachable_url: nonempty_string(frame.get("unreachableUrl")),
        loader_id: nonempty_string(frame.get("loaderId")),
    })
}

pub(super) fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn commit_transition(
    before: Option<&MainFrameState>,
    current: &MainFrameState,
    dispatch_loader_id: Option<&str>,
) -> bool {
    if let Some(dispatch_loader_id) = dispatch_loader_id {
        return current.loader_id.as_deref() == Some(dispatch_loader_id);
    }
    before.is_some_and(|before| {
        before.loader_id != current.loader_id
            || !urls_equivalent(before.committed_url(), current.committed_url())
            || before.unreachable_url != current.unreachable_url
    })
}

pub(super) fn track_transition(
    previously_seen: bool,
    current_matches_dispatch: bool,
    has_dispatch_loader: bool,
) -> bool {
    if has_dispatch_loader {
        current_matches_dispatch
    } else {
        previously_seen || current_matches_dispatch
    }
}

pub(super) fn classify_navigation(
    requested_url: &str,
    before: Option<&MainFrameState>,
    current: &MainFrameState,
    transition_seen: bool,
) -> Option<NavigationOutcome> {
    let committed_url = current.committed_url();
    if !super::super::tab_lifecycle::bindable_document_url(committed_url) {
        return None;
    }
    let direct = urls_equivalent(requested_url, committed_url);
    let already_at_destination = before
        .is_some_and(|before| urls_equivalent(requested_url, before.committed_url()) && direct);
    if !transition_seen && !already_at_destination && before.is_some() {
        return None;
    }
    if current.unreachable_url.is_some() {
        Some(NavigationOutcome::LoadFailed)
    } else if already_at_destination && !transition_seen {
        Some(NavigationOutcome::AlreadyAtDestination)
    } else if direct {
        Some(NavigationOutcome::Direct)
    } else if transition_seen {
        Some(NavigationOutcome::Redirect)
    } else {
        None
    }
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

    fn frame(url: &str, loader_id: &str) -> MainFrameState {
        MainFrameState {
            url: url.to_string(),
            unreachable_url: None,
            loader_id: Some(loader_id.to_string()),
        }
    }

    #[test]
    fn requires_exact_tab_transition_before_claiming_a_new_destination() {
        let before = frame("https://example.invalid/old", "loader-old");
        assert_eq!(
            classify_navigation("https://example.invalid/new", Some(&before), &before, false),
            None
        );

        let after = frame("https://example.invalid/new", "loader-new");
        assert_eq!(
            classify_navigation(
                "https://example.invalid/new",
                Some(&before),
                &after,
                commit_transition(Some(&before), &after, Some("loader-new")),
            ),
            Some(NavigationOutcome::Direct)
        );
    }

    #[test]
    fn accepts_redirect_only_with_same_tab_commit_evidence() {
        let before = frame("https://example.invalid/old", "loader-old");
        let redirected = frame("https://destination.invalid/article", "loader-new");
        assert_eq!(
            classify_navigation(
                "http://source.invalid/article",
                Some(&before),
                &redirected,
                true,
            ),
            Some(NavigationOutcome::Redirect)
        );
        assert_eq!(
            classify_navigation(
                "http://source.invalid/article",
                Some(&before),
                &redirected,
                false,
            ),
            None
        );

        let synthetic = frame(":", "loader-new");
        assert_eq!(
            classify_navigation(
                "http://source.invalid/article",
                Some(&before),
                &synthetic,
                true,
            ),
            None
        );
    }

    #[test]
    fn dispatch_loader_does_not_bless_an_unrelated_same_tab_navigation() {
        let before = frame("https://example.invalid/old", "loader-old");
        let unrelated = frame("https://unrelated.invalid/", "loader-unrelated");
        assert!(!commit_transition(
            Some(&before),
            &unrelated,
            Some("loader-requested")
        ));
        assert_eq!(
            classify_navigation(
                "https://requested.invalid/",
                Some(&before),
                &unrelated,
                false,
            ),
            None
        );
        assert!(!track_transition(true, false, true));
        assert!(track_transition(true, false, false));
    }

    #[test]
    fn already_satisfied_url_and_unreachable_document_are_distinct() {
        let current = frame("https://example.invalid/path", "loader-one");
        assert_eq!(
            classify_navigation(
                "https://example.invalid/path",
                Some(&current),
                &current,
                false,
            ),
            Some(NavigationOutcome::AlreadyAtDestination)
        );

        let unreachable = MainFrameState {
            url: "internal-error://document/".to_string(),
            unreachable_url: Some("https://example.invalid/path".to_string()),
            loader_id: Some("loader-two".to_string()),
        };
        assert_eq!(
            classify_navigation(
                "https://example.invalid/path",
                Some(&current),
                &unreachable,
                true,
            ),
            Some(NavigationOutcome::LoadFailed)
        );
    }

    #[test]
    fn parser_and_url_comparison_use_structured_state() {
        let state = main_frame_from_tree(&serde_json::json!({
            "frameTree": {"frame": {
                "url": "https://example.invalid/",
                "loaderId": "loader",
                "unreachableUrl": ""
            }}
        }))
        .unwrap();
        assert_eq!(state, frame("https://example.invalid/", "loader"));
        assert!(urls_equivalent(
            "https://example.invalid",
            "https://example.invalid/"
        ));
    }
}
