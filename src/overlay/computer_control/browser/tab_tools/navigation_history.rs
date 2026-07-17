//! Exact-tab browser-history traversal and committed-state verification.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::navigation::{
    NavigationDispatchMethod, navigation_dispatch_receipt, navigation_load_failure,
    navigation_success, navigation_verification_failure, read_main_frame, verify_navigation,
};
use super::navigation_state::NavigationOutcome;

const VERIFY_TIMEOUT: Duration = Duration::from_secs(5);
const VERIFY_POLL: Duration = Duration::from_millis(100);

pub(super) fn history_impl(direction: &str, tab_id: Option<i64>) -> Value {
    if let Some(result) = super::super::conn_guard() {
        return super::tag_target(super::with_effect_verified(result, false), tab_id);
    }
    let exact_tab_id = match tab_id.or_else(|| super::super::controller_io::active_tab_id().ok()) {
        Some(tab_id) => tab_id,
        None => {
            return json!({
                "ok": false,
                "code": "ERR_BROWSER_HISTORY_TARGET_UNRESOLVED",
                "error": "could not resolve one exact browser tab for history traversal",
                "dispatch_ok": false,
                "effect_verified": false,
                "effect_may_have_occurred": false,
                "executed": false,
            });
        }
    };
    let delta = match direction {
        "back" => -1_i64,
        "forward" => 1_i64,
        _ => {
            return super::tag_target(
                json!({
                    "ok": false,
                    "code": "ERR_BROWSER_HISTORY_DIRECTION_INVALID",
                    "error": "direction must be back or forward",
                    "dispatch_ok": false,
                    "effect_verified": false,
                    "effect_may_have_occurred": false,
                    "executed": false,
                }),
                Some(exact_tab_id),
            );
        }
    };
    let before = match read_main_frame(exact_tab_id) {
        Ok(before) => before,
        Err(error) => {
            return super::tag_target(preflight_failure(error), Some(exact_tab_id));
        }
    };
    let history = match super::super::bridge::cdp_on_tab(
        "Page.getNavigationHistory",
        json!({}),
        exact_tab_id,
    ) {
        Ok(history) => history,
        Err(error) => {
            return super::tag_target(preflight_failure(error), Some(exact_tab_id));
        }
    };
    let current_index = history
        .get("currentIndex")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let target_index = current_index + delta;
    let target = history
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            usize::try_from(target_index)
                .ok()
                .and_then(|index| entries.get(index))
        });
    let Some(target) = target else {
        return super::tag_target(
            json!({
                "ok": false,
                "code": "ERR_BROWSER_HISTORY_BOUNDARY",
                "error": format!("no {direction} history entry exists for this tab"),
                "dispatch_ok": false,
                "effect_verified": false,
                "effect_may_have_occurred": false,
                "executed": false,
                "current_index": current_index,
            }),
            Some(exact_tab_id),
        );
    };
    let Some(entry_id) = target.get("id").and_then(Value::as_i64) else {
        return super::tag_target(
            preflight_failure(anyhow::anyhow!("browser history entry omitted its id")),
            Some(exact_tab_id),
        );
    };
    let requested_url = target.get("url").and_then(Value::as_str).unwrap_or("");
    let started = Instant::now();
    let dispatch_result = super::super::bridge::cdp_on_tab(
        "Page.navigateToHistoryEntry",
        json!({"entryId": entry_id}),
        exact_tab_id,
    );
    let (dispatch, _) =
        navigation_dispatch_receipt(&dispatch_result, NavigationDispatchMethod::History);
    if dispatch
        .get("effect_may_have_occurred")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return super::tag_target(
            dispatch_failure_without_effect(direction, current_index, target_index, dispatch),
            Some(exact_tab_id),
        );
    }
    let index_verified = wait_for_history_index(exact_tab_id, target_index, started);
    let result = match index_verified {
        Ok(index_attempts) => {
            match verify_navigation(exact_tab_id, requested_url, Some(&before), None, started) {
                Ok(verified) if verified.outcome != NavigationOutcome::LoadFailed => {
                    let mut result = navigation_success(
                        requested_url,
                        exact_tab_id,
                        Some(&before),
                        dispatch,
                        verified,
                    );
                    result["history"] = json!({
                        "direction": direction,
                        "from_index": current_index,
                        "to_index": target_index,
                        "index_attempts": index_attempts,
                    });
                    result
                }
                Ok(verified) => navigation_load_failure(
                    requested_url,
                    exact_tab_id,
                    Some(&before),
                    dispatch,
                    verified,
                ),
                Err(failure) => navigation_verification_failure(
                    requested_url,
                    exact_tab_id,
                    Some(&before),
                    dispatch,
                    failure,
                    started.elapsed().as_millis(),
                ),
            }
        }
        Err((cancelled, attempts, last_error)) => json!({
            "ok": false,
            "code": if cancelled { "ERR_BROWSER_OPERATION_CANCELLED" } else { "ERR_BROWSER_HISTORY_NOT_COMMITTED" },
            "error": if cancelled { "browser history traversal was cancelled" } else { "the exact tab did not commit the requested history entry" },
            "cancelled": cancelled,
            "effect_verified": false,
            "effect_may_have_occurred": true,
            "target_tab_id": exact_tab_id,
            "dispatch": dispatch,
            "history": {"direction": direction, "from_index": current_index, "to_index": target_index},
            "verification": {"attempts": attempts, "last_error": last_error},
        }),
    };
    super::tag_target(result, Some(exact_tab_id))
}

fn preflight_failure(error: anyhow::Error) -> Value {
    let mut result = super::super::err(error);
    if let Some(object) = result.as_object_mut() {
        object.insert("dispatch_ok".to_string(), json!(false));
        object.insert("effect_verified".to_string(), json!(false));
        object.insert("effect_may_have_occurred".to_string(), json!(false));
        object.insert("executed".to_string(), json!(false));
        object.insert("retryable".to_string(), json!(true));
    }
    result
}

fn dispatch_failure_without_effect(
    direction: &str,
    from_index: i64,
    to_index: i64,
    dispatch: Value,
) -> Value {
    let cancelled = dispatch.get("status").and_then(Value::as_str) == Some("cancelled");
    json!({
        "ok": false,
        "code": if cancelled { "ERR_BROWSER_OPERATION_CANCELLED" } else { "ERR_BROWSER_HISTORY_NOT_DISPATCHED" },
        "error": if cancelled { "browser history traversal was cancelled before dispatch" } else { "browser history traversal was unavailable before dispatch" },
        "cancelled": cancelled,
        "dispatch_ok": false,
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
        "dispatch": dispatch,
        "history": {"direction": direction, "from_index": from_index, "to_index": to_index},
    })
}

fn wait_for_history_index(
    tab_id: i64,
    target_index: i64,
    started: Instant,
) -> Result<u32, (bool, u32, Option<String>)> {
    let mut attempts = 0_u32;
    let mut last_error = None;
    while Instant::now() < started + VERIFY_TIMEOUT {
        if super::super::readiness::action_cancelled() {
            return Err((true, attempts, last_error));
        }
        attempts = attempts.saturating_add(1);
        match super::super::bridge::cdp_on_tab("Page.getNavigationHistory", json!({}), tab_id) {
            Ok(history)
                if history.get("currentIndex").and_then(Value::as_i64) == Some(target_index) =>
            {
                return Ok(attempts);
            }
            Ok(_) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        if super::super::readiness::pause_cancelled(VERIFY_POLL) {
            return Err((true, attempts, last_error));
        }
    }
    Err((false, attempts, last_error))
}
