//! Browser tool routing that applies the user-turn's exact controlled tab.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabRoute {
    Current,
    Exact(i64),
}

fn tab_route(controlled_tab_id: Option<i64>) -> TabRoute {
    controlled_tab_id.map_or(TabRoute::Current, TabRoute::Exact)
}

fn pin_after_close(pin: Option<i64>, closed_tab_id: i64, ok: bool) -> Option<i64> {
    if ok && pin == Some(closed_tab_id) {
        None
    } else {
        pin
    }
}

fn is_document_bound_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_read_page"
            | "browser_extract_page"
            | "browser_wait_for"
            | "browser_eval"
            | "browser_navigate"
            | "browser_upload"
            | "browser_network"
            | "browser_console"
    )
}

fn requires_document_postcheck(name: &str) -> bool {
    matches!(
        name,
        "browser_read_page"
            | "browser_extract_page"
            | "browser_wait_for"
            | "browser_network"
            | "browser_console"
    )
}

fn result_provenance(name: &str) -> EvidenceProvenance {
    match name {
        "browser_eval" => EvidenceProvenance::ModelAuthoredComputation,
        _ => EvidenceProvenance::CapabilityResult,
    }
}

fn connection_requirement(name: &str) -> Option<bool> {
    match name {
        "browser_setup" | "browser_status" | "browser_reset" => Some(false),
        "browser_read_page"
        | "research_web"
        | "browser_extract_page"
        | "browser_wait_for"
        | "browser_eval"
        | "browser_navigate"
        | "browser_open_tab"
        | "browser_upload"
        | "browser_tabs"
        | "browser_switch_tab"
        | "browser_close_tab"
        | "browser_network"
        | "browser_console" => Some(true),
        _ => None,
    }
}

fn validate_document_route(
    route: TabRoute,
    document_id: Option<&str>,
    validate: impl FnOnce(i64, &str) -> anyhow::Result<()>,
) -> Option<Value> {
    let TabRoute::Exact(tab_id) = route else {
        return None;
    };
    let Some(document_id) = document_id else {
        return Some(json!({
            "ok": false,
            "code": "ERR_STALE_FRAME_SURFACE",
            "error": "the source browser document identity is unavailable",
            "target_tab_id": tab_id,
        }));
    };
    validate(tab_id, document_id).err().map(|error| {
        json!({
            "ok": false,
            "code": "ERR_STALE_FRAME_SURFACE",
            "error": error.to_string(),
            "target_tab_id": tab_id,
            "expected_document_id": document_id,
        })
    })
}

fn postcheck_document_route(
    route: TabRoute,
    document_id: Option<&str>,
    validate: impl FnOnce(i64, &str) -> anyhow::Result<()>,
    result: Value,
) -> Value {
    validate_document_route(route, document_id, validate).unwrap_or(result)
}

impl Brain {
    pub(super) fn dispatch_browser_tool(
        &mut self,
        name: &str,
        args: &Value,
        cancel: &Arc<AtomicBool>,
    ) -> Option<(Value, EvidenceProvenance)> {
        let needs_connection = connection_requirement(name)?;
        let _connection = if needs_connection {
            match super::super::browser::cancellable_connection_preflight(cancel) {
                Ok(preflight) => Some(preflight),
                Err(error) => return Some((error, EvidenceProvenance::CapabilityResult)),
            }
        } else {
            None
        };
        let route = tab_route(self.controlled_tab_id);
        if is_document_bound_tool(name)
            && let Some(error) = validate_document_route(
                route,
                self.controlled_document_id.as_deref(),
                super::super::browser::validate_document_identity_on_tab,
            )
        {
            return Some((error, EvidenceProvenance::CapabilityResult));
        }
        let result = match name {
            "browser_setup" => super::super::browser::setup(),
            "browser_status" => super::super::browser::status(),
            "browser_reset" => super::super::browser::reset(),
            "browser_read_page" => match route {
                TabRoute::Current => super::super::browser::read_page(),
                TabRoute::Exact(tab_id) => super::super::browser::read_page_on_tab(tab_id),
            },
            "research_web" => super::super::research::research_web(args),
            "browser_extract_page" => match route {
                TabRoute::Current => super::super::browser::extract_page(),
                TabRoute::Exact(tab_id) => super::super::browser::extract_page_on_tab(tab_id),
            },
            "browser_wait_for" => {
                let selector = args.get("selector").and_then(Value::as_str).unwrap_or("");
                let timeout = args
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(8000);
                match route {
                    TabRoute::Current => super::super::browser::wait_for(selector, timeout),
                    TabRoute::Exact(tab_id) => {
                        super::super::browser::wait_for_on_tab(selector, timeout, tab_id)
                    }
                }
            }
            "browser_eval" => {
                let code = args.get("code").and_then(Value::as_str).unwrap_or("");
                match route {
                    TabRoute::Current => super::super::browser::eval_js(code),
                    TabRoute::Exact(tab_id) => super::super::browser::eval_js_on_document(
                        code,
                        tab_id,
                        self.controlled_document_id.as_deref().unwrap_or(""),
                    ),
                }
            }
            "browser_navigate" => {
                let url = args.get("url").and_then(Value::as_str).unwrap_or("");
                match route {
                    TabRoute::Current => super::super::browser::navigate(url),
                    TabRoute::Exact(tab_id) => super::super::browser::navigate_on_tab(url, tab_id),
                }
            }
            "browser_open_tab" => super::super::browser::open_tab(
                args.get("url").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_upload" => {
                let selector = args.get("selector").and_then(Value::as_str).unwrap_or("");
                let path = args.get("path").and_then(Value::as_str).unwrap_or("");
                match route {
                    TabRoute::Current => super::super::browser::upload_file(selector, path),
                    TabRoute::Exact(tab_id) => super::super::browser::upload_file_on_document(
                        selector,
                        path,
                        tab_id,
                        self.controlled_document_id.as_deref().unwrap_or(""),
                    ),
                }
            }
            "browser_tabs" => super::super::browser::get_tabs(),
            "browser_switch_tab" => {
                let tab_id = args.get("tab_id").and_then(Value::as_i64).unwrap_or(0);
                let result = super::super::browser::switch_tab(tab_id);
                if result.get("ok").and_then(Value::as_bool) == Some(true) {
                    self.controlled_tab_id = Some(tab_id);
                    self.controlled_document_id = None;
                    self.controller.set_browser_tab_target(Some(tab_id));
                }
                result
            }
            "browser_close_tab" => {
                let tab_id = args.get("tab_id").and_then(Value::as_i64).unwrap_or(0);
                let result = if tab_id <= 0 {
                    json!({
                        "ok": false,
                        "code": "ERR_BROWSER_TAB_ID_REQUIRED",
                        "error": "browser_close_tab needs a positive tab_id",
                        "target_tab_id": tab_id,
                    })
                } else {
                    match super::super::browser::close_tab(tab_id) {
                        Ok(()) => json!({"ok": true, "closed": tab_id, "target_tab_id": tab_id}),
                        Err(error) => {
                            let mut result = super::super::browser::err(error);
                            if let Some(object) = result.as_object_mut() {
                                object.insert("target_tab_id".to_string(), json!(tab_id));
                            }
                            result
                        }
                    }
                };
                let ok = result.get("ok").and_then(Value::as_bool) == Some(true);
                self.controlled_tab_id = pin_after_close(self.controlled_tab_id, tab_id, ok);
                if self.controlled_tab_id.is_none() {
                    self.controlled_document_id = None;
                }
                self.controller
                    .set_browser_tab_target(self.controlled_tab_id);
                result
            }
            "browser_network" => {
                let filter = args.get("filter").and_then(Value::as_str).unwrap_or("");
                match route {
                    TabRoute::Current => super::super::browser::read_network(filter),
                    TabRoute::Exact(tab_id) => {
                        super::super::browser::read_network_on_tab(filter, tab_id)
                    }
                }
            }
            "browser_console" => match route {
                TabRoute::Current => super::super::browser::read_console(),
                TabRoute::Exact(tab_id) => super::super::browser::read_console_on_tab(tab_id),
            },
            _ => unreachable!("connection_requirement recognized the browser tool"),
        };
        let result = if requires_document_postcheck(name) {
            postcheck_document_route(
                route,
                self.controlled_document_id.as_deref(),
                super::super::browser::validate_document_identity_on_tab,
                result,
            )
        } else {
            result
        };
        Some((result, result_provenance(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_uses_exact_tab_only_when_turn_has_a_pin() {
        assert_eq!(tab_route(None), TabRoute::Current);
        assert_eq!(tab_route(Some(310)), TabRoute::Exact(310));
    }

    #[test]
    fn successful_close_clears_only_the_matching_pin() {
        assert_eq!(pin_after_close(Some(7), 7, true), None);
        assert_eq!(pin_after_close(Some(7), 8, true), Some(7));
        assert_eq!(pin_after_close(Some(7), 7, false), Some(7));
    }

    #[test]
    fn exact_page_tools_fail_closed_when_the_source_document_drifted() {
        let drift = validate_document_route(
            TabRoute::Exact(12),
            Some("doc-before"),
            |tab_id, document_id| {
                assert_eq!(tab_id, 12);
                assert_eq!(document_id, "doc-before");
                anyhow::bail!("document changed")
            },
        )
        .expect("drift must produce a typed failure");

        assert_eq!(drift["code"], "ERR_STALE_FRAME_SURFACE");
        assert_eq!(drift["target_tab_id"], 12);
        assert_eq!(drift["expected_document_id"], "doc-before");
    }

    #[test]
    fn explicit_current_route_does_not_invent_a_source_document() {
        let result = validate_document_route(TabRoute::Current, None, |_, _| {
            panic!("current route must not run an exact-document validator")
        });
        assert!(result.is_none());
        assert!(is_document_bound_tool("browser_navigate"));
        assert!(!is_document_bound_tool("browser_switch_tab"));
    }

    #[test]
    fn observational_result_is_rejected_if_navigation_wins_the_call_race() {
        let result = postcheck_document_route(
            TabRoute::Exact(22),
            Some("doc-before"),
            |_, _| anyhow::bail!("document changed after read"),
            json!({"ok": true, "page": {"text": "wrong document"}}),
        );

        assert_eq!(result["code"], "ERR_STALE_FRAME_SURFACE");
        assert!(!result.to_string().contains("wrong document"));
        assert!(requires_document_postcheck("browser_read_page"));
        assert!(!requires_document_postcheck("browser_navigate"));
    }

    #[test]
    fn model_authored_computation_is_not_direct_provider_evidence() {
        assert_eq!(
            result_provenance("browser_eval"),
            EvidenceProvenance::ModelAuthoredComputation
        );
        assert_eq!(
            result_provenance("browser_read_page"),
            EvidenceProvenance::CapabilityResult
        );
        assert_eq!(
            result_provenance("browser_extract_page"),
            EvidenceProvenance::CapabilityResult
        );
    }

    #[test]
    fn only_connection_management_tools_bypass_readiness_preflight() {
        for name in ["browser_setup", "browser_status", "browser_reset"] {
            assert_eq!(connection_requirement(name), Some(false));
        }
        for name in [
            "browser_read_page",
            "research_web",
            "browser_navigate",
            "browser_tabs",
            "browser_switch_tab",
            "browser_close_tab",
        ] {
            assert_eq!(connection_requirement(name), Some(true));
        }
        assert_eq!(connection_requirement("future_capability"), None);
    }
}
