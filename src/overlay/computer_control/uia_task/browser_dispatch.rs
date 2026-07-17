//! Browser tool routing that applies the user-turn's exact controlled tab.

use super::*;

#[path = "browser_dispatch/history.rs"]
mod history;
#[path = "browser_dispatch/managed_open.rs"]
mod managed_open;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabRoute {
    Current,
    Exact(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationPlan {
    CreateTurnTab,
    Navigate {
        route: TabRoute,
        promote_owned_lease: bool,
    },
}
fn navigation_plan(
    lifetime: super::tab_ownership::TabLifetime,
    route: TabRoute,
    route_is_turn_owned: bool,
) -> NavigationPlan {
    match (lifetime, route, route_is_turn_owned) {
        (super::tab_ownership::TabLifetime::Turn, TabRoute::Exact(tab_id), true) => {
            NavigationPlan::Navigate {
                route: TabRoute::Exact(tab_id),
                promote_owned_lease: false,
            }
        }
        (super::tab_ownership::TabLifetime::Turn, _, _) => NavigationPlan::CreateTurnTab,
        (super::tab_ownership::TabLifetime::Persistent, route, owned) => NavigationPlan::Navigate {
            route,
            promote_owned_lease: owned,
        },
    }
}
fn navigation_may_have_effect(result: &Value) -> bool {
    if result.get("ok").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    if let Some(effect) = result
        .get("effect_may_have_occurred")
        .and_then(Value::as_bool)
    {
        return effect;
    }
    if let Some(effect) = result
        .pointer("/dispatch/effect_may_have_occurred")
        .and_then(Value::as_bool)
    {
        return effect;
    }
    matches!(
        result.pointer("/dispatch/status").and_then(Value::as_str),
        Some("accepted" | "reported_error" | "unknown" | "cancelled")
    )
}

fn annotate_navigation(
    mut result: Value,
    lifetime: super::tab_ownership::TabLifetime,
    mode: &str,
    lease_promoted: bool,
) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("lifetime".to_string(), json!(lifetime.as_str()));
        object.insert("navigation_mode".to_string(), json!(mode));
        object.insert("lease_promoted".to_string(), json!(lease_promoted));
    }
    result
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

fn retire_refreshable_stale_binding(name: &str, result: &mut Value) -> bool {
    let refreshable = matches!(
        name,
        "browser_read_page"
            | "browser_extract_page"
            | "browser_wait_for"
            | "browser_network"
            | "browser_console"
    );
    let stale = result.get("code").and_then(Value::as_str) == Some("ERR_STALE_FRAME_SURFACE");
    if refreshable
        && stale
        && let Some(object) = result.as_object_mut()
    {
        object.insert("document_binding_retired".to_string(), json!(true));
    }
    refreshable && stale
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
        | "browser_history"
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
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "executed": false,
        }));
    };
    validate(tab_id, document_id).err().map(|error| {
        json!({
            "ok": false,
            "code": "ERR_STALE_FRAME_SURFACE",
            "error": error.to_string(),
            "target_tab_id": tab_id,
            "expected_document_id": document_id,
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "executed": false,
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

fn bind_successful_document(
    mut result: Value,
    tab_id: i64,
    acquire: impl FnOnce(i64) -> anyhow::Result<super::super::browser::StableDocumentIdentity>,
) -> (Value, Option<String>) {
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return (result, None);
    }
    match acquire(tab_id) {
        Ok(binding) if !binding.document_id.is_empty() && !binding.loader_id.is_empty() => {
            let document_id = binding.document_id.clone();
            if let Some(object) = result.as_object_mut() {
                object.insert("document_id".to_string(), json!(document_id.clone()));
                object.insert(
                    "document_binding".to_string(),
                    json!({
                        "loader_id": binding.loader_id,
                        "url": binding.url,
                        "stable": true,
                    }),
                );
            }
            (result, Some(document_id))
        }
        Ok(_) => (
            document_binding_failure(tab_id, result, "browser returned an empty document id"),
            None,
        ),
        Err(error) => (
            document_binding_failure(tab_id, result, &error.to_string()),
            None,
        ),
    }
}

fn document_binding_failure(tab_id: i64, effect: Value, error: &str) -> Value {
    json!({
        "ok": false,
        "code": "ERR_BROWSER_DOCUMENT_BINDING_UNAVAILABLE",
        "error": format!("the exact browser document could not be bound: {error}"),
        "target_tab_id": tab_id,
        "effect_may_have_occurred": true,
        "effect": effect,
    })
}

fn document_binding_preflight_failure(tab_id: i64, error: &str) -> Value {
    json!({
        "ok": false,
        "code": "ERR_BROWSER_DOCUMENT_BINDING_UNAVAILABLE",
        "error": format!("the exact browser document is not ready: {error}"),
        "target_tab_id": tab_id,
        "retryable": true,
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
    })
}

fn switch_requires_settled_document(activation: &Value) -> bool {
    let pending = activation
        .get("pending_url")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.is_empty());
    let committed = activation.get("url").and_then(Value::as_str);
    let transient_committed = committed
        .is_none_or(|url| url.is_empty() || url.to_ascii_lowercase().starts_with("about:blank"));
    pending || transient_committed
}

fn tab_open_error(error: anyhow::Error) -> Value {
    let ambiguous = super::super::browser::temporary_tab_open_effect_ambiguous(&error);
    let mut result = super::super::browser::err(error);
    if let Some(object) = result.as_object_mut() {
        object.insert("effect_verified".to_string(), json!(false));
        object.insert("effect_may_have_occurred".to_string(), json!(ambiguous));
        if ambiguous {
            object.insert(
                "code".to_string(),
                json!("ERR_BROWSER_TAB_CREATE_AMBIGUOUS"),
            );
        } else {
            object.insert("executed".to_string(), json!(false));
        }
    }
    result
}

impl Brain {
    fn open_browser_tab_lease(
        &mut self,
        url: &str,
        lifetime: super::tab_ownership::TabLifetime,
    ) -> Value {
        let tab = match lifetime {
            super::tab_ownership::TabLifetime::Turn => {
                super::super::browser::open_turn_owned_tab(url)
            }
            super::tab_ownership::TabLifetime::Persistent => {
                super::super::browser::open_persistent_tab(url)
            }
        };
        let tab = match tab {
            Ok(tab) => tab,
            Err(error) => return tab_open_error(error),
        };
        let created = super::super::browser::tab_lease_result(url, &tab, lifetime.as_str());
        let Some(tab_id) = created.get("target_tab_id").and_then(Value::as_i64) else {
            return created;
        };
        if lifetime == super::tab_ownership::TabLifetime::Turn {
            self.turn_tabs.track(tab);
        }
        self.controlled_tab_id = Some(tab_id);
        self.controlled_document_id = None;
        self.controller.set_browser_tab_target(Some(tab_id));
        let (result, document_id) = bind_successful_document(created, tab_id, |tab_id| {
            super::super::browser::await_readable_document_on_tab(tab_id)
        });
        if result.get("ok").and_then(Value::as_bool) == Some(true) {
            self.controlled_document_id = document_id;
        }
        result
    }

    fn bind_navigation_result(&mut self, result: Value, tab_id: i64) -> Value {
        let navigation_succeeded = result.get("ok").and_then(Value::as_bool) == Some(true);
        let effect_may_have_occurred = navigation_may_have_effect(&result);
        let expected_loader_id = result
            .pointer("/verification/loader_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let (result, document_id) = bind_successful_document(result, tab_id, |tab_id| {
            super::super::browser::acquire_stable_document_identity_on_tab(
                tab_id,
                expected_loader_id.as_deref(),
                false,
            )
        });
        if navigation_succeeded {
            self.controlled_document_id = document_id;
        } else if effect_may_have_occurred {
            self.controlled_document_id = None;
        }
        result
    }

    pub(super) fn dispatch_browser_tool(
        &mut self,
        name: &str,
        args: &Value,
        cancel: &Arc<AtomicBool>,
    ) -> Option<Value> {
        if let Some(result) = managed_open::dispatch(self, name, args, cancel) {
            return Some(result);
        }
        let needs_connection = connection_requirement(name)?;
        let _connection = if needs_connection {
            match super::super::browser::cancellable_connection_preflight(cancel) {
                Ok(preflight) => Some(preflight),
                Err(error) => return Some(error),
            }
        } else {
            None
        };
        let route = managed_open::tab_route(self.controlled_tab_id);
        if is_document_bound_tool(name)
            && let TabRoute::Exact(tab_id) = route
            && self.controlled_document_id.is_none()
        {
            match super::super::browser::await_readable_document_on_tab(tab_id) {
                Ok(binding) => self.controlled_document_id = Some(binding.document_id),
                Err(error) => {
                    return Some(document_binding_preflight_failure(
                        tab_id,
                        &error.to_string(),
                    ));
                }
            }
        }
        if is_document_bound_tool(name)
            && let Some(mut error) = validate_document_route(
                route,
                self.controlled_document_id.as_deref(),
                super::super::browser::validate_document_identity_on_tab,
            )
        {
            if retire_refreshable_stale_binding(name, &mut error) {
                self.controlled_document_id = None;
            }
            return Some(error);
        }
        let result = match name {
            "browser_setup" => super::super::browser::setup(),
            "browser_status" => super::super::browser::status(),
            "browser_reset" => {
                self.retire_owned_tabs(super::tab_ownership::RetirementReason::Completed);
                super::super::browser::reset()
            }
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
                let lifetime = match super::tab_ownership::TabLifetime::parse_required(
                    args,
                    "browser_navigate",
                ) {
                    Ok(lifetime) => lifetime,
                    Err(error) => return Some(error),
                };
                let url = args.get("url").and_then(Value::as_str).unwrap_or("");
                let owned = match route {
                    TabRoute::Exact(tab_id) => self.turn_tabs.owns(tab_id),
                    TabRoute::Current => false,
                };
                match navigation_plan(lifetime, route, owned) {
                    NavigationPlan::CreateTurnTab => annotate_navigation(
                        self.open_browser_tab_lease(url, super::tab_ownership::TabLifetime::Turn),
                        lifetime,
                        "created_turn_tab",
                        false,
                    ),
                    NavigationPlan::Navigate {
                        route,
                        promote_owned_lease,
                    } => {
                        let result = match route {
                            TabRoute::Current => super::super::browser::navigate(url),
                            TabRoute::Exact(tab_id) => {
                                super::super::browser::navigate_on_tab(url, tab_id)
                            }
                        };
                        let promoted = promote_owned_lease
                            && navigation_may_have_effect(&result)
                            && match route {
                                TabRoute::Exact(tab_id) => self.turn_tabs.promote(tab_id),
                                TabRoute::Current => false,
                            };
                        let target_tab_id = match route {
                            TabRoute::Exact(tab_id) => Some(tab_id),
                            TabRoute::Current => result
                                .get("target_tab_id")
                                .and_then(Value::as_i64)
                                .filter(|tab_id| *tab_id > 0),
                        };
                        let result = if let Some(tab_id) = target_tab_id {
                            self.controlled_tab_id = Some(tab_id);
                            self.controller.set_browser_tab_target(Some(tab_id));
                            self.bind_navigation_result(result, tab_id)
                        } else {
                            result
                        };
                        let mode = if lifetime == super::tab_ownership::TabLifetime::Turn {
                            "existing_turn_tab"
                        } else {
                            "persistent_target"
                        };
                        annotate_navigation(result, lifetime, mode, promoted)
                    }
                }
            }
            "browser_history" => history::dispatch(self, args, route),
            "browser_open_tab" => {
                let lifetime = match super::tab_ownership::TabLifetime::parse(args) {
                    Ok(lifetime) => lifetime,
                    Err(error) => return Some(error),
                };
                let url = args.get("url").and_then(Value::as_str).unwrap_or("");
                self.open_browser_tab_lease(url, lifetime)
            }
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
                let activation = super::super::browser::switch_tab(tab_id);
                let activated = activation.get("ok").and_then(Value::as_bool) == Some(true);
                let require_settled_tab = switch_requires_settled_document(&activation);
                let (result, document_id) =
                    bind_successful_document(activation, tab_id, |tab_id| {
                        super::super::browser::acquire_stable_document_identity_on_tab(
                            tab_id,
                            None,
                            require_settled_tab,
                        )
                    });
                if activated {
                    self.controlled_tab_id = Some(tab_id);
                    self.controlled_document_id = document_id;
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
                        "retryable": true,
                        "effect_verified": false,
                        "effect_may_have_occurred": false,
                        "executed": false,
                    })
                } else {
                    super::super::browser::close_tab_checked(tab_id)
                };
                let ok = result.get("ok").and_then(Value::as_bool) == Some(true);
                // A turn lease remains registered after an explicit close so
                // retirement independently verifies that the tab is absent.
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
        let mut result = if requires_document_postcheck(name) {
            postcheck_document_route(
                route,
                self.controlled_document_id.as_deref(),
                super::super::browser::validate_document_identity_on_tab,
                result,
            )
        } else {
            result
        };
        if retire_refreshable_stale_binding(name, &mut result) {
            self.controlled_document_id = None;
        }
        Some(result)
    }
}

#[cfg(test)]
#[path = "browser_dispatch/tests.rs"]
mod tests;
