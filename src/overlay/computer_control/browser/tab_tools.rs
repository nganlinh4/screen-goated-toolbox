//! Browser tools that can target either the extension's current tab or one exact
//! tab id pinned by the owning user turn.

use serde_json::{Value, json};
use std::time::Duration;

use super::bridge;

mod navigation;
mod navigation_history;
mod navigation_state;
#[cfg(test)]
mod tests;

fn tag_target(mut result: Value, tab_id: Option<i64>) -> Value {
    if let (Some(tab_id), Some(object)) = (tab_id, result.as_object_mut()) {
        object.insert("target_tab_id".to_string(), json!(tab_id));
    }
    result
}

fn cdp(tab_id: Option<i64>, method: &str, params: Value) -> anyhow::Result<Value> {
    match tab_id {
        Some(tab_id) => bridge::cdp_on_tab(method, params, tab_id),
        None => bridge::cdp(method, params),
    }
}

fn eval(tab_id: Option<i64>, code: &str) -> anyhow::Result<Value> {
    match tab_id {
        Some(tab_id) => super::eval_value_on_tab(code, tab_id),
        None => super::eval_value(code),
    }
}

pub(in crate::overlay::computer_control) fn eval_js(code: &str) -> Value {
    eval_js_impl(code, None)
}

pub(in crate::overlay::computer_control) fn eval_js_on_document(
    code: &str,
    tab_id: i64,
    expected_document_id: &str,
) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, Some(tab_id));
    }
    let result = match super::document_binding::eval_main_world(tab_id, expected_document_id, code)
    {
        Ok(super::document_binding::BoundEvaluation::Value(value)) => match value.get("result") {
            Some(result) => evaluation_result(result.clone()),
            None => json!({
                "ok": false,
                "code": "ERR_BROWSER_TOOL_FAILED",
                "error": "js expression returned undefined; explicitly return a JSON-compatible value",
                "effect_may_have_occurred": true,
            }),
        },
        Ok(super::document_binding::BoundEvaluation::Stale(observed)) => json!({
            "ok": false,
            "code": "ERR_STALE_FRAME_SURFACE",
            "stale": true,
            "effect_may_have_occurred": false,
            "error": "browser document changed before JavaScript dispatch",
            "expected_document_id": expected_document_id,
            "observed_document_id": observed,
        }),
        Ok(super::document_binding::BoundEvaluation::DispatchedError(error)) => {
            super::errors::uncertain_dispatch(error)
        }
        Err(error) => super::err(error),
    };
    tag_target(result, Some(tab_id))
}

fn eval_js_impl(code: &str, tab_id: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, tab_id);
    }
    let result = match eval(tab_id, code) {
        Ok(value) => evaluation_result(value),
        Err(error) => super::err(error),
    };
    tag_target(result, tab_id)
}

fn evaluation_result(value: Value) -> Value {
    json!({
        "ok": true,
        "result": value,
        "structured_receipt": {
            "operation_complete": true,
            "desktop_grounding": "not_applicable",
        },
    })
}

pub(in crate::overlay::computer_control) fn wait_for(selector: &str, timeout_ms: u64) -> Value {
    wait_for_impl(selector, timeout_ms, None)
}

pub(in crate::overlay::computer_control) fn wait_for_on_tab(
    selector: &str,
    timeout_ms: u64,
    tab_id: i64,
) -> Value {
    wait_for_impl(selector, timeout_ms, Some(tab_id))
}

fn wait_for_impl(selector: &str, timeout_ms: u64, tab_id: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, tab_id);
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.min(30_000));
    let js = format!(
        "(() => !!document.querySelector({selector}))()",
        selector = json!(selector)
    );
    loop {
        match eval(tab_id, &js) {
            Ok(Value::Bool(true)) => {
                return tag_target(json!({"ok": true, "found": selector}), tab_id);
            }
            Ok(_) => {}
            Err(_) if super::readiness::action_cancelled() => {
                return tag_target(cancelled_result("browser_wait_for", false), tab_id);
            }
            Err(error) => return tag_target(super::err(error), tab_id),
        }
        if std::time::Instant::now() > deadline {
            return tag_target(
                json!({"ok": false, "error": format!("'{selector}' not found within {timeout_ms}ms")}),
                tab_id,
            );
        }
        if super::readiness::pause_cancelled(Duration::from_millis(200)) {
            return tag_target(cancelled_result("browser_wait_for", false), tab_id);
        }
    }
}

pub(in crate::overlay::computer_control) fn navigate(url: &str) -> Value {
    navigation::navigate_impl(url, None)
}

pub(in crate::overlay::computer_control) fn navigate_on_tab(url: &str, tab_id: i64) -> Value {
    navigation::navigate_impl(url, Some(tab_id))
}

pub(in crate::overlay::computer_control) fn traverse_history(direction: &str) -> Value {
    navigation_history::history_impl(direction, None)
}

pub(in crate::overlay::computer_control) fn traverse_history_on_tab(
    direction: &str,
    tab_id: i64,
) -> Value {
    navigation_history::history_impl(direction, Some(tab_id))
}

fn with_effect_verified(mut result: Value, verified: bool) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("effect_verified".to_string(), json!(verified));
    }
    result
}

fn cancelled_result(stage: &str, effect_may_have_occurred: bool) -> Value {
    json!({
        "ok": false,
        "code": "ERR_BROWSER_OPERATION_CANCELLED",
        "status": "aborted_by_user",
        "cancelled": true,
        "stage": stage,
        "effect_may_have_occurred": effect_may_have_occurred,
    })
}

pub(in crate::overlay::computer_control) fn read_network(filter: &str) -> Value {
    read_network_impl(filter, None)
}

pub(in crate::overlay::computer_control) fn read_network_on_tab(
    filter: &str,
    tab_id: i64,
) -> Value {
    read_network_impl(filter, Some(tab_id))
}

fn read_network_impl(filter: &str, route: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, route);
    }
    let tab_id = match event_scope(route, super::active_tab_id) {
        Ok(tab_id) => tab_id,
        Err(error) => return tag_target(super::err(error), route),
    };
    if let Err(error) = cdp(Some(tab_id), "Network.enable", json!({})) {
        return tag_target(super::err(error), Some(tab_id));
    }
    tag_target(
        json!({
            "ok": true,
            "events": network_items(filter, tab_id),
            "note": "Network capture is enabled; call again after the page makes requests."
        }),
        Some(tab_id),
    )
}

/// The one tab a page-scoped read enables capture on and reads events from.
/// The current route resolves the active tab once; the retained event ring
/// holds every attached tab's events, so an unscoped read would mix them.
fn event_scope(
    route: Option<i64>,
    active_tab: impl FnOnce() -> anyhow::Result<i64>,
) -> anyhow::Result<i64> {
    route.map_or_else(active_tab, Ok)
}

fn network_items(filter: &str, tab_id: i64) -> Vec<Value> {
    let wanted = if filter.is_empty() {
        "Network."
    } else {
        filter
    };
    bridge::recent_events_on_tab(wanted, 30, tab_id)
        .iter()
        .map(|event| {
            let params = event.get("params").cloned().unwrap_or_else(|| json!({}));
            json!({
                "tab_id": event.get("tabId"),
                "method": event.get("method"),
                "omitted": event.get("event_omitted").and_then(Value::as_bool).unwrap_or(false),
                "url": params.get("response").and_then(|value| value.get("url"))
                    .or_else(|| params.get("request").and_then(|value| value.get("url"))),
                "status": params.get("response").and_then(|value| value.get("status")),
            })
        })
        .collect()
}

/// Page-controlled console text returned to the model, per entry.
const MAX_CONSOLE_TEXT_CHARS: usize = 2000;

fn bounded_console_text(text: &str) -> (String, bool) {
    match text.char_indices().nth(MAX_CONSOLE_TEXT_CHARS) {
        Some((end, _)) => (text[..end].to_string(), true),
        None => (text.to_string(), false),
    }
}

fn event_omitted(event: &Value) -> bool {
    event
        .get("event_omitted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn console_api_item(event: &Value) -> Value {
    let params = event.get("params").cloned().unwrap_or_else(|| json!({}));
    let text = params
        .get("args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .map(|arg| {
                    arg.get("value")
                        .and_then(Value::as_str)
                        .or_else(|| arg.get("description").and_then(Value::as_str))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let (text, text_truncated) = bounded_console_text(&text);
    json!({
        "tab_id": event.get("tabId"),
        "level": params.get("type"), "text": text, "text_truncated": text_truncated,
        "omitted": event_omitted(event),
    })
}

fn log_entry_item(event: &Value) -> Value {
    let entry = event
        .get("params")
        .and_then(|params| params.get("entry"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let field =
        |name| bounded_console_text(entry.get(name).and_then(Value::as_str).unwrap_or_default());
    let (text, text_truncated) = field("text");
    let (url, _) = field("url");
    json!({
        "tab_id": event.get("tabId"),
        "level": entry.get("level"),
        "text": text,
        "text_truncated": text_truncated,
        "url": url,
        "omitted": event_omitted(event),
    })
}

pub(in crate::overlay::computer_control) fn read_console() -> Value {
    read_console_impl(None)
}

pub(in crate::overlay::computer_control) fn read_console_on_tab(tab_id: i64) -> Value {
    read_console_impl(Some(tab_id))
}

fn read_console_impl(route: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, route);
    }
    let tab_id = match event_scope(route, super::active_tab_id) {
        Ok(tab_id) => tab_id,
        Err(error) => return tag_target(super::err(error), route),
    };
    for method in ["Runtime.enable", "Log.enable"] {
        if let Err(error) = cdp(Some(tab_id), method, json!({})) {
            return tag_target(super::err(error), Some(tab_id));
        }
    }
    tag_target(
        json!({
            "ok": true,
            "console": console_items(tab_id),
            "note": "Console and browser-log capture are enabled; call again after the page emits events."
        }),
        Some(tab_id),
    )
}

fn console_items(tab_id: i64) -> Vec<Value> {
    bridge::recent_events_on_tab("consoleAPICalled", 25, tab_id)
        .iter()
        .map(console_api_item)
        .chain(
            bridge::recent_events_on_tab("Log.entryAdded", 25, tab_id)
                .iter()
                .map(log_entry_item),
        )
        .collect()
}
