//! Controller-only evaluation in a browser-owned isolated JavaScript world.

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::controller_io::MutationRoute;

const WORLD_NAME: &str = "sgt-computer-control-identity-v1";

/// Keep the identity map outside the page's JavaScript world. DOM attributes
/// remain shared and are only lookup hints; node identity is held in WeakMap.
pub(in crate::overlay::computer_control) const DOCUMENT_ID_JS: &str = r#"(() => {
    const key = Symbol.for('sgt.controller.private-state');
    if (!globalThis[key]) {
        const id = (globalThis.crypto && crypto.randomUUID)
            ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`;
        globalThis[key] = { documentId: id, nodes: new WeakMap() };
    }
    return globalThis[key].documentId;
})()"#;

pub(in crate::overlay::computer_control) fn eval(
    route: MutationRoute,
    expression: &str,
    tab_id: i64,
    session_id: Option<&str>,
) -> Result<Value> {
    let context = context(route, tab_id, session_id)?;
    let response = route.cdp(
        "Runtime.evaluate",
        json!({
            "expression": expression, "contextId": context,
            "returnByValue": true, "awaitPromise": true,
        }),
        session_id,
        tab_id,
    )?;
    if let Some(exception) = response.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", super::js_exception_text(exception));
    }
    super::runtime_result_value(&response)
}

pub(super) fn context(route: MutationRoute, tab_id: i64, session_id: Option<&str>) -> Result<i64> {
    let tree = route.cdp("Page.getFrameTree", json!({}), session_id, tab_id)?;
    let frame = tree
        .pointer("/frameTree/frame/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .context("controller frame identity unavailable")?;
    let world = route.cdp(
        "Page.createIsolatedWorld",
        json!({
            "frameId": frame, "worldName": WORLD_NAME,
        }),
        session_id,
        tab_id,
    )?;
    world
        .get("executionContextId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .context("controller isolated world unavailable")
}
