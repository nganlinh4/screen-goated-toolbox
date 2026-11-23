//! Bind main-world execution to the exact document object sampled in an isolated world.

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::controller_io::MutationRoute;

pub(super) const MAIN_WORLD_CALL: &str =
    "async function(code) { return {guard: 'ok', result: await (0, eval)(code)}; }";

pub(super) enum BoundEvaluation {
    Stale(Option<String>),
    Value(Value),
    DispatchedError(anyhow::Error),
}

/// Resolve one DOM root into both worlds. Navigation invalidates its remote
/// object, so `callFunctionOn` cannot silently execute in the replacement page.
pub(super) fn eval_main_world(tab_id: i64, expected: &str, code: &str) -> Result<BoundEvaluation> {
    let route = MutationRoute::Exact;
    let context = super::controller_isolated::context(route, tab_id, None)?;
    let document = route.cdp("DOM.getDocument", json!({"depth": 0}), None, tab_id)?;
    let backend_id = document
        .pointer("/root/backendNodeId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .context("browser document root identity unavailable")?;
    let isolated = route.cdp(
        "DOM.resolveNode",
        json!({"backendNodeId": backend_id, "executionContextId": context}),
        None,
        tab_id,
    )?;
    let isolated_id = object_id(&isolated)?;
    let verify = route.cdp(
        "Runtime.callFunctionOn",
        json!({
            "objectId": isolated_id,
            "functionDeclaration": format!(
                "function() {{ return this === document ? ({}) : null; }}",
                super::controller_isolated::DOCUMENT_ID_JS
            ),
            "returnByValue": true,
        }),
        None,
        tab_id,
    );
    let _ = route.cdp(
        "Runtime.releaseObject",
        json!({"objectId": isolated_id}),
        None,
        tab_id,
    );
    let verify = verify?;
    if let Some(exception) = verify.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", super::js_exception_text(exception));
    }
    let observed = verify.pointer("/result/value").and_then(Value::as_str);
    if observed != Some(expected) {
        return Ok(BoundEvaluation::Stale(observed.map(str::to_string)));
    }
    let main = route.cdp(
        "DOM.resolveNode",
        json!({"backendNodeId": backend_id}),
        None,
        tab_id,
    )?;
    let main_id = object_id(&main)?;
    let result = route.cdp(
        "Runtime.callFunctionOn",
        json!({
            "objectId": main_id, "functionDeclaration": MAIN_WORLD_CALL,
            "arguments": [{"value": code}], "returnByValue": true, "awaitPromise": true,
        }),
        None,
        tab_id,
    );
    let _ = route.cdp(
        "Runtime.releaseObject",
        json!({"objectId": main_id}),
        None,
        tab_id,
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => return Ok(BoundEvaluation::DispatchedError(error)),
    };
    if let Some(exception) = result.get("exceptionDetails") {
        return Ok(BoundEvaluation::DispatchedError(anyhow::anyhow!(
            "js exception: {}",
            super::js_exception_text(exception)
        )));
    }
    Ok(match super::runtime_result_value(&result) {
        Ok(value) => BoundEvaluation::Value(value),
        Err(error) => BoundEvaluation::DispatchedError(error),
    })
}

fn object_id(value: &Value) -> Result<&str> {
    value
        .pointer("/object/objectId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .context("browser document object unavailable")
}
