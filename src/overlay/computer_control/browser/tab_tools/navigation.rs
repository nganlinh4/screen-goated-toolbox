//! Exact-tab navigation dispatch and committed-main-frame verification.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::navigation_state::{
    MainFrameState, NavigationOutcome, classify_navigation, commit_transition,
    main_frame_from_tree, nonempty_string, track_transition,
};

const VERIFY_TIMEOUT: Duration = Duration::from_secs(5);
const VERIFY_POLL: Duration = Duration::from_millis(100);
const STABLE_SAMPLES: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NavigationDispatchMethod {
    Cdp,
    ExactTabRpc,
    History,
}

impl NavigationDispatchMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cdp => "cdp.Page.navigate",
            Self::ExactTabRpc => "tabs.navigate",
            Self::History => "cdp.Page.navigateToHistoryEntry",
        }
    }
}

#[derive(Debug)]
pub(super) struct NavigationVerification {
    pub(super) outcome: NavigationOutcome,
    pub(super) state: MainFrameState,
    pub(super) attempts: u32,
    pub(super) elapsed_ms: u128,
}

pub(super) fn navigate_impl(url: &str, tab_id: Option<i64>) -> Value {
    let url = match super::super::tab_lifecycle::required_url(url, "browser_navigate") {
        Ok(url) => url,
        Err(error) => {
            return super::tag_target(super::with_effect_verified(error, false), tab_id);
        }
    };
    if let Some(result) = super::super::conn_guard() {
        return super::tag_target(super::with_effect_verified(result, false), tab_id);
    }
    let exact_tab_id = match tab_id {
        Some(tab_id) => tab_id,
        None => match super::super::controller_io::active_tab_id() {
            Ok(tab_id) => tab_id,
            Err(error) => {
                return json!({
                    "ok": false,
                    "effect_verified": false,
                    "code": "ERR_BROWSER_NAVIGATION_TARGET_UNRESOLVED",
                    "error": "could not resolve one exact browser tab before navigation",
                    "requested_url": url,
                    "dispatch": {"status": "not_attempted"},
                    "verification": {
                        "status": "inconclusive",
                        "reason": error.to_string(),
                    },
                });
            }
        },
    };
    if super::super::readiness::action_cancelled() {
        return super::tag_target(
            super::cancelled_result("browser_navigation_dispatch", false),
            Some(exact_tab_id),
        );
    }

    let mut before = read_main_frame(exact_tab_id).ok();
    if super::super::readiness::action_cancelled() {
        return super::tag_target(
            super::cancelled_result("browser_navigation_dispatch", false),
            Some(exact_tab_id),
        );
    }
    let dispatch_method = navigation_dispatch_method(
        before.is_some(),
        super::super::capabilities::supports(super::super::capabilities::TABS_NAVIGATE),
    );
    let dispatch_result = match dispatch_method {
        NavigationDispatchMethod::Cdp => {
            super::super::bridge::cdp_on_tab("Page.navigate", json!({"url": url}), exact_tab_id)
        }
        NavigationDispatchMethod::ExactTabRpc => super::super::bridge::rpc(
            "tabs",
            json!({"action": "navigate", "tabId": exact_tab_id, "url": url}),
        ),
        NavigationDispatchMethod::History => unreachable!("URL navigation never uses history"),
    };
    if before.is_none() && dispatch_method == NavigationDispatchMethod::ExactTabRpc {
        before = dispatch_result
            .as_ref()
            .ok()
            .and_then(tab_navigation_before_state);
    }
    let (dispatch, dispatch_loader_id) =
        navigation_dispatch_receipt(&dispatch_result, dispatch_method);
    if dispatch
        .get("effect_may_have_occurred")
        .and_then(Value::as_bool)
        == Some(false)
    {
        let cancelled = dispatch.get("status").and_then(Value::as_str) == Some("cancelled");
        return super::tag_target(
            json!({
                "ok": false,
                "code": if cancelled { "ERR_BROWSER_OPERATION_CANCELLED" } else { "ERR_BROWSER_NAVIGATION_NOT_DISPATCHED" },
                "error": if cancelled { "browser navigation was cancelled before dispatch" } else { "browser navigation was unavailable before dispatch" },
                "cancelled": cancelled,
                "dispatch_ok": false,
                "effect_verified": false,
                "effect_may_have_occurred": false,
                "executed": false,
                "requested_url": url,
                "dispatch": dispatch,
            }),
            Some(exact_tab_id),
        );
    }
    let started = Instant::now();
    let verification = verify_navigation(
        exact_tab_id,
        url,
        before.as_ref(),
        dispatch_loader_id.as_deref(),
        started,
    );

    let result = match verification {
        Ok(verified) if verified.outcome != NavigationOutcome::LoadFailed => {
            navigation_success(url, exact_tab_id, before.as_ref(), dispatch, verified)
        }
        Ok(verified) => {
            navigation_load_failure(url, exact_tab_id, before.as_ref(), dispatch, verified)
        }
        Err(failure) => navigation_verification_failure(
            url,
            exact_tab_id,
            before.as_ref(),
            dispatch,
            failure,
            started.elapsed().as_millis(),
        ),
    };
    super::tag_target(result, Some(exact_tab_id))
}

pub(super) fn navigation_dispatch_receipt(
    result: &anyhow::Result<Value>,
    method: NavigationDispatchMethod,
) -> (Value, Option<String>) {
    match result {
        Ok(value) if method == NavigationDispatchMethod::ExactTabRpc => (
            json!({
                "status": "accepted",
                "method": method.as_str(),
                "tab_id": value.get("id").and_then(Value::as_i64),
                "before_url": value.get("beforeUrl").and_then(Value::as_str),
                "reported_url": value.get("url").and_then(Value::as_str),
                "pending_url": value.get("pendingUrl").and_then(Value::as_str),
            }),
            None,
        ),
        Ok(value) => {
            let loader_id = nonempty_string(value.get("loaderId"));
            let error_text = nonempty_string(value.get("errorText"));
            let status = if error_text.is_some() {
                "reported_error"
            } else {
                "accepted"
            };
            (
                json!({
                    "status": status,
                    "method": method.as_str(),
                    "frame_id": nonempty_string(value.get("frameId")),
                    "loader_id": loader_id.clone(),
                    "error_text": error_text,
                    "is_download": value.get("isDownload").and_then(Value::as_bool).unwrap_or(false),
                }),
                loader_id,
            )
        }
        Err(error) => {
            let cancelled = super::super::bridge_wait::cancellation_effect(error).is_some();
            let effect = super::super::bridge_wait::dispatch_effect(error)
                .unwrap_or_else(|| super::super::capabilities::unsupported_from(error).is_none());
            (
                json!({
                    "status": if cancelled { "cancelled" } else if effect { "unknown" } else { "not_dispatched" },
                    "method": method.as_str(),
                    "error": error.to_string(),
                    "effect_may_have_occurred": effect,
                }),
                None,
            )
        }
    }
}

pub(super) fn navigation_dispatch_method(
    cdp_before_available: bool,
    exact_tab_rpc_available: bool,
) -> NavigationDispatchMethod {
    if !cdp_before_available && exact_tab_rpc_available {
        NavigationDispatchMethod::ExactTabRpc
    } else {
        NavigationDispatchMethod::Cdp
    }
}

pub(super) fn tab_navigation_before_state(value: &Value) -> Option<MainFrameState> {
    let url = nonempty_string(value.get("beforeUrl"))?;
    Some(MainFrameState {
        url,
        unreachable_url: None,
        loader_id: None,
    })
}

pub(super) fn verify_navigation(
    tab_id: i64,
    requested_url: &str,
    before: Option<&MainFrameState>,
    dispatch_loader_id: Option<&str>,
    started: Instant,
) -> Result<NavigationVerification, VerificationFailure> {
    let deadline = started + VERIFY_TIMEOUT;
    let mut attempts = 0_u32;
    let mut last_state = None;
    let mut stable_samples = 0_u8;
    let mut last_error = None;
    let mut transition_seen = false;
    let mut cancelled = false;

    loop {
        if super::super::readiness::action_cancelled() {
            cancelled = true;
            break;
        }
        attempts = attempts.saturating_add(1);
        match read_main_frame(tab_id) {
            Ok(state) => {
                let current_matches_dispatch =
                    commit_transition(before, &state, dispatch_loader_id);
                transition_seen = track_transition(
                    transition_seen,
                    current_matches_dispatch,
                    dispatch_loader_id.is_some(),
                );
                if last_state.as_ref() == Some(&state) {
                    stable_samples = stable_samples.saturating_add(1);
                } else {
                    stable_samples = 1;
                }
                last_state = Some(state.clone());
                if stable_samples >= STABLE_SAMPLES
                    && let Some(outcome) =
                        classify_navigation(requested_url, before, &state, transition_seen)
                {
                    return Ok(NavigationVerification {
                        outcome,
                        state,
                        attempts,
                        elapsed_ms: started.elapsed().as_millis(),
                    });
                }
            }
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            break;
        }
        if super::super::readiness::pause_cancelled(VERIFY_POLL) {
            cancelled = true;
            break;
        }
    }

    Err(VerificationFailure {
        last_state,
        last_error,
        attempts,
        transition_seen,
        cancelled,
    })
}

#[derive(Debug)]
pub(super) struct VerificationFailure {
    pub(super) last_state: Option<MainFrameState>,
    pub(super) last_error: Option<String>,
    pub(super) attempts: u32,
    pub(super) transition_seen: bool,
    pub(super) cancelled: bool,
}

pub(super) fn read_main_frame(tab_id: i64) -> anyhow::Result<MainFrameState> {
    let tree = super::super::bridge::cdp_on_tab("Page.getFrameTree", json!({}), tab_id)?;
    main_frame_from_tree(&tree)
}

pub(super) fn navigation_success(
    requested_url: &str,
    tab_id: i64,
    before: Option<&MainFrameState>,
    dispatch: Value,
    verified: NavigationVerification,
) -> Value {
    let committed_url = verified.state.committed_url().to_string();
    let loader_id = verified.state.loader_id.clone();
    json!({
        "ok": true,
        "effect_verified": true,
        "navigated": committed_url,
        "requested_url": requested_url,
        "committed_url": committed_url,
        "redirected": verified.outcome == NavigationOutcome::Redirect,
        "target_tab_id": tab_id,
        "dispatch": dispatch,
        "verification": {
            "status": "committed",
            "outcome": verified.outcome.as_str(),
            "method": "exact_tab_main_frame",
            "before_url": before.map(MainFrameState::committed_url),
            "committed_url": committed_url,
            "loader_id": loader_id,
            "attempts": verified.attempts,
            "elapsed_ms": verified.elapsed_ms,
        },
    })
}

pub(super) fn navigation_load_failure(
    requested_url: &str,
    tab_id: i64,
    before: Option<&MainFrameState>,
    dispatch: Value,
    verified: NavigationVerification,
) -> Value {
    let committed_url = verified.state.committed_url().to_string();
    let document_url = verified.state.url.clone();
    let unreachable_url = verified.state.unreachable_url.clone();
    json!({
        "ok": false,
        "effect_verified": false,
        "code": "ERR_BROWSER_NAVIGATION_LOAD_FAILED",
        "error": "the exact target tab committed an unreachable error document",
        "requested_url": requested_url,
        "committed_url": committed_url,
        "target_tab_id": tab_id,
        "dispatch": dispatch,
        "verification": {
            "status": "committed_error_page",
            "outcome": verified.outcome.as_str(),
            "method": "exact_tab_main_frame",
            "before_url": before.map(MainFrameState::committed_url),
            "document_url": document_url,
            "unreachable_url": unreachable_url,
            "attempts": verified.attempts,
            "elapsed_ms": verified.elapsed_ms,
        },
    })
}

pub(super) fn navigation_verification_failure(
    requested_url: &str,
    tab_id: i64,
    before: Option<&MainFrameState>,
    dispatch: Value,
    failure: VerificationFailure,
    elapsed_ms: u128,
) -> Value {
    if failure.cancelled {
        let effect_may_have_occurred = dispatch
            .get("effect_may_have_occurred")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        return json!({
            "ok": false,
            "effect_verified": false,
            "effect_may_have_occurred": effect_may_have_occurred,
            "code": "ERR_BROWSER_OPERATION_CANCELLED",
            "status": "aborted_by_user",
            "cancelled": true,
            "stage": "browser_navigation_verification",
            "requested_url": requested_url,
            "target_tab_id": tab_id,
            "dispatch": dispatch,
            "verification": {
                "status": "cancelled",
                "method": "exact_tab_main_frame",
                "attempts": failure.attempts,
                "elapsed_ms": elapsed_ms,
            },
        });
    }
    let status = if failure.last_state.is_some() && !failure.transition_seen {
        "not_committed"
    } else {
        "inconclusive"
    };
    let code = if status == "not_committed" {
        "ERR_BROWSER_NAVIGATION_NOT_COMMITTED"
    } else {
        "ERR_BROWSER_NAVIGATION_VERIFICATION_INCONCLUSIVE"
    };
    json!({
        "ok": false,
        "effect_verified": false,
        "code": code,
        "error": "the exact target tab did not expose a stable committed destination before the verification deadline",
        "requested_url": requested_url,
        "target_tab_id": tab_id,
        "dispatch": dispatch,
        "verification": {
            "status": status,
            "method": "exact_tab_main_frame",
            "before_url": before.map(MainFrameState::committed_url),
            "observed_url": failure.last_state.as_ref().map(MainFrameState::committed_url),
            "last_error": failure.last_error,
            "transition_seen": failure.transition_seen,
            "attempts": failure.attempts,
            "elapsed_ms": elapsed_ms,
        },
    })
}
