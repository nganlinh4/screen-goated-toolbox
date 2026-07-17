//! File-input mutation guarded by exact tab, document, and DOM-node identities.

use serde_json::{Value, json};
use std::path::Path;

use super::controller_io::{DOCUMENT_ID_JS, MutationRoute};

#[derive(Clone, Debug, PartialEq, Eq)]
struct UploadTarget {
    document_id: String,
    backend_node_id: i64,
    node_id: i64,
}

impl UploadTarget {
    fn stale_reason(&self, current: &Self) -> Option<&'static str> {
        if self.document_id != current.document_id {
            Some("document_changed")
        } else if self.backend_node_id != current.backend_node_id {
            Some("element_changed")
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
struct UploadRoute {
    mode: MutationRoute,
    tab_id: i64,
    pinned: bool,
}

impl UploadRoute {
    fn resolve(pinned_tab_id: Option<i64>) -> anyhow::Result<Self> {
        match pinned_tab_id {
            Some(tab_id) => Ok(Self {
                mode: MutationRoute::Exact,
                tab_id,
                pinned: true,
            }),
            None => Ok(Self {
                mode: MutationRoute::Active,
                tab_id: super::active_tab_id()?,
                pinned: false,
            }),
        }
    }

    fn tag(self, mut value: Value) -> Value {
        if let Some(object) = value.as_object_mut() {
            object.insert("target_tab_id".to_string(), json!(self.tab_id));
            object.insert("target_pinned".to_string(), json!(self.pinned));
        }
        value
    }
}

pub(in crate::overlay::computer_control) fn upload_file(selector: &str, path: &str) -> Value {
    upload_file_impl(selector, path, None)
}

pub(in crate::overlay::computer_control) fn upload_file_on_document(
    selector: &str,
    path: &str,
    tab_id: i64,
    document_id: &str,
) -> Value {
    upload_file_impl_on_document(selector, path, Some(tab_id), Some(document_id))
}

fn upload_file_impl(selector: &str, path: &str, pinned_tab_id: Option<i64>) -> Value {
    upload_file_impl_on_document(selector, path, pinned_tab_id, None)
}

fn upload_file_impl_on_document(
    selector: &str,
    path: &str,
    pinned_tab_id: Option<i64>,
    source_document_id: Option<&str>,
) -> Value {
    if let Some(error) = invalid_upload_path(path) {
        return error;
    }
    if let Some(mut result) = super::conn_guard() {
        if let (Some(tab_id), Some(object)) = (pinned_tab_id, result.as_object_mut()) {
            object.insert("target_tab_id".to_string(), json!(tab_id));
            object.insert("target_pinned".to_string(), json!(true));
        }
        return result;
    }
    let route = match UploadRoute::resolve(pinned_tab_id) {
        Ok(route) => route,
        Err(error) => return super::err(error),
    };
    let expected = match resolve_target(route, selector) {
        Ok(Some(target)) => target,
        Ok(None) => {
            return route.tag(json!({
                "ok": false,
                "code": "ERR_BROWSER_TARGET_NOT_FOUND",
                "error": format!("no element matches {selector}"),
            }));
        }
        Err(error) => return route.tag(super::err(error)),
    };
    if let Some(document_id) = source_document_id
        && document_id != expected.document_id
    {
        return stale_source_document(route, selector, document_id, &expected);
    }
    let current = match resolve_target(route, selector) {
        Ok(Some(target)) => target,
        Ok(None) => return stale_upload(route, selector, "target_missing", &expected, None),
        Err(error) => {
            return stale_upload(
                route,
                selector,
                &format!("identity_unavailable: {error}"),
                &expected,
                None,
            );
        }
    };
    if let Some(reason) = expected.stale_reason(&current) {
        return stale_upload(route, selector, reason, &expected, Some(&current));
    }
    if let Some(document_id) = source_document_id
        && document_id != current.document_id
    {
        return stale_source_document(route, selector, document_id, &current);
    }
    let dispatch = route.mode.cdp(
        "DOM.setFileInputFiles",
        json!({"nodeId": current.node_id, "files": [path]}),
        None,
        route.tab_id,
    );
    let after_document = dispatch
        .as_ref()
        .ok()
        .and(source_document_id)
        .and_then(|_| route.mode.eval(DOCUMENT_ID_JS, route.tab_id, None).ok())
        .and_then(|value| value.as_str().map(str::to_string));
    match dispatch {
        Ok(_)
            if source_document_id
                .is_some_and(|expected| after_document.as_deref() != Some(expected)) =>
        {
            route.tag(json!({
                "ok": false,
                "code": "ERR_STALE_FRAME_SURFACE",
                "stale": true,
                "effect_may_have_occurred": true,
                "error": "browser document changed while the file input mutation was dispatched",
                "expected_document_id": source_document_id,
                "observed_document_id": after_document,
            }))
        }
        Ok(_) => route.tag(json!({
            "ok": true,
            "uploaded": path,
            "document_guard": "matched",
            "element_guard": "matched",
        })),
        Err(error) => route.tag(super::err(error)),
    }
}

fn invalid_upload_path(path: &str) -> Option<Value> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Some(json!({
            "ok": false,
            "code": "ERR_BROWSER_UPLOAD_PATH_NOT_ABSOLUTE",
            "error": "upload path must be absolute",
            "effect_may_have_occurred": false,
        }));
    }
    if !path.is_file() {
        return Some(json!({
            "ok": false,
            "code": "ERR_BROWSER_UPLOAD_FILE_UNAVAILABLE",
            "error": "upload path must name an existing file",
            "effect_may_have_occurred": false,
        }));
    }
    None
}

fn stale_source_document(
    route: UploadRoute,
    selector: &str,
    expected_document_id: &str,
    observed: &UploadTarget,
) -> Value {
    route.tag(json!({
        "ok": false,
        "code": "ERR_STALE_FRAME_SURFACE",
        "stale": true,
        "dispatch_ok": false,
        "effect_may_have_occurred": false,
        "error": "the file input no longer belongs to the model-visible document",
        "reason": "document_changed",
        "phase": "before_set_file_input_files",
        "selector": selector,
        "expected_document_id": expected_document_id,
        "observed_document_id": observed.document_id,
    }))
}

fn resolve_target(route: UploadRoute, selector: &str) -> anyhow::Result<Option<UploadTarget>> {
    let document_id = route
        .mode
        .eval(DOCUMENT_ID_JS, route.tab_id, None)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("browser document identity unavailable"))?;
    let document = route
        .mode
        .cdp("DOM.getDocument", json!({"depth": 0}), None, route.tab_id)?;
    let root = document
        .pointer("/root/nodeId")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("browser DOM root unavailable"))?;
    let query = route.mode.cdp(
        "DOM.querySelector",
        json!({"nodeId": root, "selector": selector}),
        None,
        route.tab_id,
    )?;
    let node_id = query.get("nodeId").and_then(Value::as_i64).unwrap_or(0);
    if node_id == 0 {
        return Ok(None);
    }
    let described = route.mode.cdp(
        "DOM.describeNode",
        json!({"nodeId": node_id}),
        None,
        route.tab_id,
    )?;
    let backend_node_id = described
        .pointer("/node/backendNodeId")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("browser DOM node identity unavailable"))?;
    Ok(Some(UploadTarget {
        document_id,
        backend_node_id,
        node_id,
    }))
}

fn stale_upload(
    route: UploadRoute,
    selector: &str,
    reason: &str,
    expected: &UploadTarget,
    observed: Option<&UploadTarget>,
) -> Value {
    route.tag(json!({
        "ok": false,
        "code": "ERR_BROWSER_STALE_TARGET",
        "stale": true,
        "dispatch_ok": false,
        "effect_may_have_occurred": false,
        "error": "the browser document or file input changed before upload; inspect it again",
        "reason": reason,
        "phase": "before_set_file_input_files",
        "selector": selector,
        "expected": {
            "document_id": expected.document_id,
            "backend_node_id": expected.backend_node_id,
        },
        "observed": observed.map(|target| json!({
            "document_id": target.document_id,
            "backend_node_id": target.backend_node_id,
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::{UploadTarget, invalid_upload_path};
    use std::fs;

    fn target(document: &str, backend_node_id: i64) -> UploadTarget {
        UploadTarget {
            document_id: document.to_string(),
            backend_node_id,
            node_id: backend_node_id + 100,
        }
    }

    #[test]
    fn upload_rejects_navigation_or_replaced_file_input() {
        let expected = target("document-a", 11);
        assert_eq!(
            expected.stale_reason(&target("document-b", 11)),
            Some("document_changed")
        );
        assert_eq!(
            expected.stale_reason(&target("document-a", 12)),
            Some("element_changed")
        );
        assert_eq!(expected.stale_reason(&target("document-a", 11)), None);
    }

    #[test]
    fn upload_rejects_relative_and_non_file_paths_before_browser_dispatch() {
        let relative = invalid_upload_path("fixture.txt").unwrap();
        assert_eq!(relative["code"], "ERR_BROWSER_UPLOAD_PATH_NOT_ABSOLUTE");
        assert_eq!(relative["effect_may_have_occurred"], false);

        let directory =
            std::env::temp_dir().join(format!("sgt-browser-upload-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let not_file = invalid_upload_path(directory.to_str().unwrap()).unwrap();
        fs::remove_dir(&directory).unwrap();
        assert_eq!(not_file["code"], "ERR_BROWSER_UPLOAD_FILE_UNAVAILABLE");
        assert_eq!(not_file["effect_may_have_occurred"], false);
    }
}
