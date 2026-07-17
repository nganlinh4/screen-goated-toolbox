//! Browser-controller I/O pinned to one active tab. This is separate from the
//! general browser tools because indexed actions must fail closed on tab changes.

use serde_json::{Value, json};

use super::{bridge, err, js_exception_text};

pub(in crate::overlay::computer_control) const DOCUMENT_ID_JS: &str = r#"(() => {
    const key = Symbol.for('sgt.controller.document-id');
    if (!globalThis[key]) {
        globalThis[key] = (globalThis.crypto && crypto.randomUUID)
            ? crypto.randomUUID()
            : `${Date.now()}-${Math.random()}`;
    }
    return globalThis[key];
})()"#;

#[derive(Clone, Copy)]
pub(super) enum MutationRoute {
    Active,
    Exact,
}

impl MutationRoute {
    pub(super) fn eval(
        self,
        expression: &str,
        tab_id: i64,
        session_id: Option<&str>,
    ) -> anyhow::Result<Value> {
        match self {
            Self::Active => eval_value_in_active_tab(expression, tab_id, session_id),
            Self::Exact => super::eval_value_in_exact_tab(expression, tab_id, session_id),
        }
    }

    pub(super) fn cdp(
        self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        tab_id: i64,
    ) -> anyhow::Result<Value> {
        match self {
            Self::Active => active_tab_cdp(method, params, session_id, tab_id),
            Self::Exact => bridge::cdp_in_tab(method, params, session_id, tab_id),
        }
    }
}

#[derive(Debug, PartialEq)]
struct TargetSnapshot {
    x: f64,
    y: f64,
    focused: bool,
}

#[derive(Clone, Copy)]
struct TargetExpectation<'a> {
    selector: &'a str,
    tab_id: i64,
    document_id: &'a str,
    element_id: &'a str,
}

fn stale_target(
    selector: &str,
    tab_id: i64,
    phase: &str,
    reason: &str,
    expected_document_id: &str,
    expected_element_id: &str,
    observed: Option<&Value>,
) -> Value {
    json!({
        "ok": false,
        "code": "ERR_BROWSER_STALE_TARGET",
        "stale": true,
        "dispatch_ok": false,
        "effect_may_have_occurred": false,
        "error": "the browser document or element changed before input dispatch; observe again",
        "reason": reason,
        "phase": phase,
        "selector": selector,
        "target_tab_id": tab_id,
        "expected": {
            "document_id": expected_document_id,
            "element_id": expected_element_id,
        },
        "observed": observed.cloned().unwrap_or(Value::Null),
    })
}

fn classify_target_snapshot(
    value: &Value,
    selector: &str,
    tab_id: i64,
    phase: &str,
    expected_document_id: &str,
    expected_element_id: &str,
    require_focus: bool,
) -> std::result::Result<TargetSnapshot, Value> {
    let document_id = value.get("documentId").and_then(Value::as_str);
    let element_id = value.get("elementId").and_then(Value::as_str);
    let reason = if document_id != Some(expected_document_id) {
        Some("document_changed")
    } else if value.get("present").and_then(Value::as_bool) != Some(true) {
        Some("target_missing")
    } else if element_id != Some(expected_element_id) {
        Some("element_changed")
    } else if value.get("interactable").and_then(Value::as_bool) != Some(true) {
        Some("target_not_interactable")
    } else if require_focus && value.get("focused").and_then(Value::as_bool) != Some(true) {
        Some("focus_changed")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(stale_target(
            selector,
            tab_id,
            phase,
            reason,
            expected_document_id,
            expected_element_id,
            Some(value),
        ));
    }
    let x = value.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = value.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    Ok(TargetSnapshot {
        x,
        y,
        focused: value.get("focused").and_then(Value::as_bool) == Some(true),
    })
}

/// Identity of the active tab in the browser's last-focused window. Extensions
/// that prove focused-window ownership answer directly and fail closed — a failed
/// proof must never degrade to title matching, which is not ownership evidence.
/// Older staged extensions cannot make that proof and fall back to the tab list
/// plus the foreground window title, failing on ambiguity.
pub(in crate::overlay::computer_control) fn active_tab_id() -> anyhow::Result<i64> {
    if super::capabilities::supports(super::capabilities::TABS_ACTIVE_FOCUSED_WINDOW) {
        return proven_active_tab(&bridge::rpc("tabs", json!({"action": "active"}))?);
    }
    let snapshot = super::super::uia::input_target_snapshot();
    let foreground = snapshot.get("title").and_then(Value::as_str).unwrap_or("");
    if let Ok(value) = bridge::rpc("tabs", json!({"action": "active"}))
        && value
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|title| title_matches_window(title, foreground))
        && let Some(id) = value.get("id").and_then(Value::as_i64)
    {
        return Ok(id);
    }
    let tabs = bridge::rpc("tabs", json!({"action": "list"}))?;
    select_active_tab(&tabs, foreground)
        .ok_or_else(|| anyhow::anyhow!("active browser tab identity is ambiguous"))
}

fn proven_active_tab(value: &Value) -> anyhow::Result<i64> {
    if value.get("windowFocused").and_then(Value::as_bool) != Some(true) {
        anyhow::bail!("active browser window is not OS-focused");
    }
    value
        .get("id")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or_else(|| anyhow::anyhow!("active browser surface omitted id"))
}

fn select_active_tab(tabs: &Value, foreground_title: &str) -> Option<i64> {
    let active: Vec<_> = tabs
        .as_array()?
        .iter()
        .filter(|tab| tab.get("active").and_then(Value::as_bool) == Some(true))
        .collect();
    let matching: Vec<_> = active
        .into_iter()
        .filter(|tab| {
            tab.get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| title_matches_window(title, foreground_title))
        })
        .collect();
    match matching.as_slice() {
        [tab] => tab.get("id").and_then(Value::as_i64),
        _ => None,
    }
}

pub(super) fn title_matches_window(tab_title: &str, window_title: &str) -> bool {
    let tab_title = tab_title.trim();
    if tab_title.is_empty() {
        return false;
    }
    window_title == tab_title
        || window_title
            .strip_prefix(tab_title)
            .is_some_and(|suffix| suffix.starts_with(" - ") || suffix.starts_with(" — "))
}

pub(in crate::overlay::computer_control) fn eval_value_in_active_tab(
    expression: &str,
    tab_id: i64,
    session_id: Option<&str>,
) -> anyhow::Result<Value> {
    let params = json!({"expression": expression, "returnByValue": true, "awaitPromise": true});
    let result = if super::capabilities::supports(super::capabilities::CDP_REQUIRE_ACTIVE) {
        bridge::cdp_in_active_tab("Runtime.evaluate", params, session_id, tab_id)?
    } else {
        ensure_active_tab(tab_id)?;
        let result = bridge::cdp_in_tab("Runtime.evaluate", params, session_id, tab_id)?;
        ensure_active_tab(tab_id)?;
        result
    };
    if let Some(exception) = result.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", js_exception_text(exception));
    }
    super::runtime_result_value(&result)
}

pub(in crate::overlay::computer_control) fn click_selector_on_active_tab(
    selector: &str,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
) -> Value {
    click_selector_impl(
        selector,
        tab_id,
        expected_document_id,
        expected_element_id,
        MutationRoute::Active,
    )
}

pub(in crate::overlay::computer_control) fn click_selector_on_tab(
    selector: &str,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
) -> Value {
    click_selector_impl(
        selector,
        tab_id,
        expected_document_id,
        expected_element_id,
        MutationRoute::Exact,
    )
}

fn click_selector_impl(
    selector: &str,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
    route: MutationRoute,
) -> Value {
    if let Some(value) = super::conn_guard() {
        return value;
    }
    let expected = TargetExpectation {
        selector,
        tab_id,
        document_id: expected_document_id,
        element_id: expected_element_id,
    };
    // Resolve once after scroll/layout settlement, then cross the activation
    // boundary with one atomic gesture. A separate hover move creates a
    // self-induced TOCTOU window on pages that replace controls on hover.
    let target = match inspect_target(route, expected, None, false, "before_activation") {
        Ok(snapshot) => snapshot,
        Err(error) => return error,
    };
    let (method, params) = atomic_activation(target.x, target.y);
    if let Err(error) = route.cdp(method, params, None, tab_id) {
        return err(error);
    }
    json!({
        "ok": true,
        "clicked": [target.x.round(), target.y.round()],
        "tab_id": tab_id,
        "input_contract": "atomic_activation",
        "document_guard": "matched",
        "element_guard": "matched",
    })
}

fn atomic_activation(x: f64, y: f64) -> (&'static str, Value) {
    (
        "Input.synthesizeTapGesture",
        json!({"x":x,"y":y,"duration":50,"tapCount":1,"gestureSourceType":"mouse"}),
    )
}

fn selector_center_expression(selector: &str) -> String {
    format!(
        r#"(async () => {{ const documentId = ({document_id});
            const initial = document.querySelector({selector});
            if (!initial) return {{documentId, present:false}};
            initial.scrollIntoView({{block:'center', inline:'center', behavior:'instant'}});
            await new Promise((resolve) => requestAnimationFrame(
                () => requestAnimationFrame(resolve)));
            const e = document.querySelector({selector});
            if (!e) return {{documentId, present:false}};
            const elementId = e[Symbol.for('sgt.controller.element-id')] || null;
            const r = e.getBoundingClientRect();
            const x = r.left + r.width / 2;
            const y = r.top + r.height / 2;
            const viewportWidth = document.documentElement?.clientWidth || innerWidth || 0;
            const viewportHeight = document.documentElement?.clientHeight || innerHeight || 0;
            const interactable = r.width > 0 && r.height > 0 &&
                Number.isFinite(x) && Number.isFinite(y) &&
                x >= 0 && y >= 0 && x < viewportWidth && y < viewportHeight;
            return {{documentId, elementId, present:true,
                focused:e===document.activeElement || e.contains(document.activeElement),
                interactable, x, y, width:r.width, height:r.height,
                viewportWidth, viewportHeight}}; }})()"#,
        selector = json!(selector),
        document_id = DOCUMENT_ID_JS,
    )
}

pub(in crate::overlay::computer_control) fn fill_in_on_active_tab(
    selector: &str,
    text: &str,
    session: Option<&str>,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
) -> Value {
    fill_in_impl(
        selector,
        text,
        session,
        tab_id,
        expected_document_id,
        expected_element_id,
        MutationRoute::Active,
    )
}

pub(in crate::overlay::computer_control) fn fill_in_on_tab(
    selector: &str,
    text: &str,
    session: Option<&str>,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
) -> Value {
    fill_in_impl(
        selector,
        text,
        session,
        tab_id,
        expected_document_id,
        expected_element_id,
        MutationRoute::Exact,
    )
}

fn fill_in_impl(
    selector: &str,
    text: &str,
    session: Option<&str>,
    tab_id: i64,
    expected_document_id: &str,
    expected_element_id: &str,
    route: MutationRoute,
) -> Value {
    if let Some(value) = super::conn_guard() {
        return value;
    }
    let expected = TargetExpectation {
        selector,
        tab_id,
        document_id: expected_document_id,
        element_id: expected_element_id,
    };
    let focus = format!(
        r#"(() => {{ const documentId = ({document_id});
            const e=document.querySelector({selector});
            if(!e) return {{documentId,present:false}};
            const elementId=e[Symbol.for('sgt.controller.element-id')] || null;
            e.focus(); if(e.select) e.select();
            return {{documentId,elementId,present:true,
                focused:e===document.activeElement || e.contains(document.activeElement)}}; }})()"#,
        selector = json!(selector),
        document_id = DOCUMENT_ID_JS,
    );
    let focused = match route.eval(&focus, tab_id, session) {
        Ok(value) => value,
        Err(error) => {
            return stale_target(
                selector,
                tab_id,
                "focus",
                &format!("identity_unavailable: {error}"),
                expected_document_id,
                expected_element_id,
                None,
            );
        }
    };
    if let Err(error) = classify_target_snapshot(
        &focused,
        selector,
        tab_id,
        "focus",
        expected_document_id,
        expected_element_id,
        true,
    ) {
        return error;
    }
    // Focus can be stolen by page script after focus()/select(). Check the same
    // document, element, and active focus immediately before trusted text input.
    if let Err(error) = inspect_target(route, expected, session, true, "before_insert_text") {
        return error;
    }
    match route.cdp("Input.insertText", json!({"text": text}), session, tab_id) {
        Ok(_) => json!({
            "ok": true,
            "filled": selector,
            "tab_id": tab_id,
            "document_guard": "matched",
            "element_guard": "matched",
            "focus_guard": "matched",
        }),
        Err(error) => err(error),
    }
}

fn inspect_target(
    route: MutationRoute,
    expected: TargetExpectation<'_>,
    session: Option<&str>,
    require_focus: bool,
    phase: &str,
) -> std::result::Result<TargetSnapshot, Value> {
    let value = route
        .eval(
            &selector_center_expression(expected.selector),
            expected.tab_id,
            session,
        )
        .map_err(|error| {
            stale_target(
                expected.selector,
                expected.tab_id,
                phase,
                &format!("identity_unavailable: {error}"),
                expected.document_id,
                expected.element_id,
                None,
            )
        })?;
    classify_target_snapshot(
        &value,
        expected.selector,
        expected.tab_id,
        phase,
        expected.document_id,
        expected.element_id,
        require_focus,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveDispatchMode {
    ExtensionGuard,
    VerifiedExplicitTab,
}

fn active_dispatch_mode() -> anyhow::Result<ActiveDispatchMode> {
    select_active_dispatch_mode(
        super::capabilities::supports(super::capabilities::CDP_REQUIRE_ACTIVE),
        super::capabilities::supports(super::capabilities::CDP_EXPLICIT_TAB),
    )
    .ok_or_else(|| super::capabilities::unsupported(super::capabilities::CDP_EXPLICIT_TAB))
}

fn select_active_dispatch_mode(
    extension_guard: bool,
    explicit_tab: bool,
) -> Option<ActiveDispatchMode> {
    if extension_guard {
        Some(ActiveDispatchMode::ExtensionGuard)
    } else if explicit_tab {
        Some(ActiveDispatchMode::VerifiedExplicitTab)
    } else {
        None
    }
}

fn ensure_active_tab(tab_id: i64) -> anyhow::Result<()> {
    if active_tab_id()? == tab_id {
        Ok(())
    } else {
        anyhow::bail!("target browser tab is no longer active")
    }
}

fn active_tab_cdp(
    method: &str,
    params: Value,
    session_id: Option<&str>,
    tab_id: i64,
) -> anyhow::Result<Value> {
    match active_dispatch_mode()? {
        ActiveDispatchMode::ExtensionGuard => {
            bridge::cdp_in_active_tab(method, params, session_id, tab_id)
        }
        ActiveDispatchMode::VerifiedExplicitTab => {
            // Legacy protocols cannot make the active check and CDP dispatch one
            // extension-side operation. Bracket the exact-tab command as tightly
            // as possible; the small check/dispatch race is limited to legacy.
            ensure_active_tab(tab_id)?;
            let result = bridge::cdp_in_tab(method, params, session_id, tab_id)?;
            ensure_active_tab(tab_id)?;
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests;
