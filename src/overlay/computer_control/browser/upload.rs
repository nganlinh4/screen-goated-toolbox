//! File-input mutation guarded by exact tab, document, and DOM-node identities.

use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::controller_io::MutationRoute;
mod checkpoint;
mod file_lease;
mod proposal;

#[derive(Clone, Debug, PartialEq, Eq)]
struct UploadTarget {
    document_id: String,
    backend_node_id: i64,
    node_id: i64,
    url: String,
    origin: String,
}

impl UploadTarget {
    fn stale_reason(&self, current: &Self) -> Option<&'static str> {
        if self.document_id != current.document_id {
            Some("document_changed")
        } else if self.url != current.url || self.origin != current.origin {
            Some("receiver_changed")
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

pub(in crate::overlay::computer_control) fn upload_file(
    selector: &str,
    path: &str,
    pinned_tab_id: Option<i64>,
    source_document_id: Option<&str>,
    cancel: &AtomicBool,
    authorize: impl FnOnce(&Value) -> Result<Value, Value>,
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
    let file = match file_lease::FileLease::open(path, cancel) {
        Ok(file) => file,
        Err(error) => {
            return route.tag(preflight_error(
                "ERR_BROWSER_UPLOAD_FILE_UNAVAILABLE",
                &error.to_string(),
            ));
        }
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
    let proposal = match expected.proposal(route.tab_id, selector, &file) {
        Ok(proposal) => proposal,
        Err(error) => {
            return route.tag(preflight_error(
                "ERR_BROWSER_UPLOAD_PROPOSAL_UNAVAILABLE",
                &error.to_string(),
            ));
        }
    };
    let disclosed_path = match file.path() {
        Ok(path) => path,
        Err(error) => {
            return route.tag(preflight_error(
                "ERR_BROWSER_UPLOAD_FILE_UNAVAILABLE",
                &error.to_string(),
            ));
        }
    };
    route.tag(checkpoint::before_disclosure(
        &proposal,
        cancel,
        authorize,
        || {
            file.validate(cancel).map_err(|error| {
                preflight_error("ERR_BROWSER_UPLOAD_FILE_CHANGED", &error.to_string())
            })?;
            revalidate_target(route, selector, source_document_id, &expected)
        },
        |current| dispatch_file(route, selector, disclosed_path, &current),
    ))
}

fn revalidate_target(
    route: UploadRoute,
    selector: &str,
    source_document_id: Option<&str>,
    expected: &UploadTarget,
) -> Result<UploadTarget, Value> {
    let current = match resolve_target(route, selector) {
        Ok(Some(target)) => target,
        Ok(None) => {
            return Err(stale_upload(
                route,
                selector,
                "target_missing",
                expected,
                None,
            ));
        }
        Err(error) => {
            return Err(stale_upload(
                route,
                selector,
                &format!("identity_unavailable: {error}"),
                expected,
                None,
            ));
        }
    };
    if let Some(reason) = expected.stale_reason(&current) {
        return Err(stale_upload(
            route,
            selector,
            reason,
            expected,
            Some(&current),
        ));
    }
    if let Some(document_id) = source_document_id
        && document_id != current.document_id
    {
        return Err(stale_source_document(
            route,
            selector,
            document_id,
            &current,
        ));
    }
    Ok(current)
}

fn dispatch_file(route: UploadRoute, selector: &str, path: &str, current: &UploadTarget) -> Value {
    let dispatch = route.mode.cdp(
        "DOM.setFileInputFiles",
        json!({"backendNodeId": current.backend_node_id, "files": [path]}),
        None,
        route.tab_id,
    );
    match dispatch {
        Ok(_) => {
            let after = resolve_target(route, selector);
            match after {
                Ok(Some(after)) if current.stale_reason(&after).is_none() => route.tag(json!({
                    "ok": true,
                    "uploaded": path,
                    "file_input_assignment": "accepted",
                    "receiving_origin": current.origin,
                    "document_guard": "matched",
                    "element_guard": "matched",
                })),
                _ => route.tag(json!({
                    "ok": false,
                    "code": "ERR_BROWSER_STALE_TARGET",
                    "stale": true,
                    "effect_may_have_occurred": true,
                    "error": "the file input or its document changed during upload; inspect it before retrying",
                })),
            }
        }
        Err(error) => route.tag(super::errors::uncertain_dispatch(error)),
    }
}

fn preflight_error(code: &str, message: &str) -> Value {
    json!({"ok":false,"code":code,"error":message,"executed":false,"effect_may_have_occurred":false})
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
    let document_id = document_identity(route)?;
    let (url, origin) = receiver_location(route)?;
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
    validate_file_input(&described["node"])?;
    let current_location = receiver_location(route)?;
    anyhow::ensure!(
        current_location.0 == url && current_location.1 == origin,
        "browser receiving location changed while resolving the file input"
    );
    if document_identity(route)? != document_id {
        anyhow::bail!("browser document changed while resolving the file input");
    }
    Ok(Some(UploadTarget {
        document_id,
        backend_node_id,
        node_id,
        url,
        origin,
    }))
}

fn receiver_location(route: UploadRoute) -> anyhow::Result<(String, String)> {
    let tree = route
        .mode
        .cdp("Page.getFrameTree", json!({}), None, route.tab_id)?;
    let frame = tree
        .pointer("/frameTree/frame")
        .ok_or_else(|| anyhow::anyhow!("receiving browser frame unavailable"))?;
    let url = super::frame_url::from_frame(frame)?;
    let origin = frame
        .get("securityOrigin")
        .and_then(Value::as_str)
        .filter(|origin| !origin.is_empty() && origin.len() <= 4096 && !origin.contains('\0'))
        .ok_or_else(|| anyhow::anyhow!("receiving browser origin unavailable"))?
        .to_string();
    Ok((url, origin))
}

fn validate_file_input(node: &Value) -> anyhow::Result<()> {
    let is_input = node.get("localName").and_then(Value::as_str) == Some("input");
    let attributes = node
        .get("attributes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("file input attributes unavailable"))?;
    let (attributes, remainder) = attributes.as_chunks::<2>();
    anyhow::ensure!(remainder.is_empty(), "file input attributes malformed");
    let is_file = attributes.iter().any(|attribute| {
        attribute[0].as_str() == Some("type")
            && attribute[1]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case("file"))
    });
    anyhow::ensure!(is_input && is_file, "upload target must be a file input");
    Ok(())
}

fn document_identity(route: UploadRoute) -> anyhow::Result<String> {
    super::controller_isolated::eval(
        route.mode,
        super::controller_isolated::DOCUMENT_ID_JS,
        route.tab_id,
        None,
    )?
    .as_str()
    .map(str::to_string)
    .ok_or_else(|| anyhow::anyhow!("browser document identity unavailable"))
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
            url: "https://receiver.example/upload".into(),
            origin: "https://receiver.example".into(),
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
        let mut moved = expected.clone();
        moved.url.push_str("?different-route");
        assert_eq!(expected.stale_reason(&moved), Some("receiver_changed"));
        moved = expected.clone();
        moved.origin = "null".into();
        assert_eq!(expected.stale_reason(&moved), Some("receiver_changed"));
        moved = expected.clone();
        moved.node_id += 1;
        assert_eq!(expected.stale_reason(&moved), None);
    }

    #[test]
    fn upload_requires_structural_file_input_attributes() {
        use serde_json::json;
        assert!(
            super::validate_file_input(&json!({"localName":"input","attributes":["type","FiLe"]}))
                .is_ok()
        );
        for node in [
            json!({"localName":"div","attributes":["type","file"]}),
            json!({"localName":"input","attributes":["type","text"]}),
            json!({"localName":"input","attributes":[]}),
            json!({"localName":"input","attributes":["type","file","unpaired"]}),
            json!({"localName":"input"}),
        ] {
            assert!(super::validate_file_input(&node).is_err());
        }
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
