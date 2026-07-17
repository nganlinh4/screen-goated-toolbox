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
    let code = match serde_json::to_string(code) {
        Ok(code) => code,
        Err(error) => return tag_target(super::err(error.into()), Some(tab_id)),
    };
    let expected_document_json = match serde_json::to_string(expected_document_id) {
        Ok(document_id) => document_id,
        Err(error) => return tag_target(super::err(error.into()), Some(tab_id)),
    };
    let expression = format!(
        r#"(async () => {{
            const documentId = ({document_identity});
            if (documentId !== {expected_document_json}) {{
                return {{guard: 'stale', documentId}};
            }}
            return {{guard: 'ok', result: await (0, eval)({code})}};
        }})()"#,
        document_identity = super::DOCUMENT_ID_JS,
    );
    let result = match eval(Some(tab_id), &expression) {
        Ok(value) if value.get("guard").and_then(Value::as_str) == Some("ok") => {
            match value.get("result") {
                Some(result) => json!({
                    "ok": true,
                    "result": result,
                    "structured_receipt": {
                        "operation_complete": true,
                        "desktop_grounding": "not_applicable",
                    },
                }),
                None => json!({
                    "ok": false,
                    "code": "ERR_BROWSER_TOOL_FAILED",
                    "error": "js expression returned undefined; explicitly return a JSON-compatible value",
                }),
            }
        }
        Ok(value) => json!({
            "ok": false,
            "code": "ERR_STALE_FRAME_SURFACE",
            "stale": true,
            "effect_may_have_occurred": false,
            "error": "browser document changed before JavaScript dispatch",
            "expected_document_id": expected_document_id,
            "observed_document_id": value.get("documentId"),
        }),
        Err(error) => super::err(error),
    };
    tag_target(result, Some(tab_id))
}

fn eval_js_impl(code: &str, tab_id: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, tab_id);
    }
    let result = match eval(tab_id, code) {
        Ok(value) => json!({
            "ok": true,
            "result": value,
            "structured_receipt": {
                "operation_complete": true,
                "desktop_grounding": "not_applicable",
            },
        }),
        Err(error) => super::err(error),
    };
    tag_target(result, tab_id)
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

fn read_network_impl(filter: &str, tab_id: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, tab_id);
    }
    if let Err(error) = cdp(tab_id, "Network.enable", json!({})) {
        return tag_target(super::err(error), tab_id);
    }
    let wanted = if filter.is_empty() {
        "Network."
    } else {
        filter
    };
    let events = bridge::recent_events_on_tab(wanted, 30, tab_id)
        .iter()
        .map(|event| {
            let params = event.get("params").cloned().unwrap_or_else(|| json!({}));
            json!({
                "method": event.get("method"),
                "url": params.get("response").and_then(|value| value.get("url"))
                    .or_else(|| params.get("request").and_then(|value| value.get("url"))),
                "status": params.get("response").and_then(|value| value.get("status")),
            })
        })
        .collect::<Vec<_>>();
    tag_target(
        json!({
            "ok": true,
            "events": events,
            "note": "Network capture is enabled; call again after the page makes requests."
        }),
        tab_id,
    )
}

pub(in crate::overlay::computer_control) fn read_console() -> Value {
    read_console_impl(None)
}

pub(in crate::overlay::computer_control) fn read_console_on_tab(tab_id: i64) -> Value {
    read_console_impl(Some(tab_id))
}

fn read_console_impl(tab_id: Option<i64>) -> Value {
    if let Some(result) = super::conn_guard() {
        return tag_target(result, tab_id);
    }
    for method in ["Runtime.enable", "Log.enable"] {
        if let Err(error) = cdp(tab_id, method, json!({})) {
            return tag_target(super::err(error), tab_id);
        }
    }
    let mut items = Vec::new();
    for event in bridge::recent_events_on_tab("consoleAPICalled", 25, tab_id) {
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
        items.push(json!({"level": params.get("type"), "text": text}));
    }
    for event in bridge::recent_events_on_tab("Log.entryAdded", 25, tab_id) {
        let entry = event
            .get("params")
            .and_then(|params| params.get("entry"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        items.push(json!({
            "level": entry.get("level"),
            "text": entry.get("text"),
            "url": entry.get("url"),
        }));
    }
    tag_target(
        json!({
            "ok": true,
            "console": items,
            "note": "Console and browser-log capture are enabled; call again after the page emits events."
        }),
        tab_id,
    )
}
