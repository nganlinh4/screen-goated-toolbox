//! Structural tab-tool contracts independent of Chrome's transient load state.

use serde_json::{Value, json};
use std::time::{Duration, Instant};

const MAX_VALID_TAB_CHOICES: usize = 64;
const CLOSE_VERIFY_BUDGET: Duration = Duration::from_millis(1200);
const CLOSE_VERIFY_POLL: Duration = Duration::from_millis(50);

pub(super) fn bindable_document_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    !(url.scheme() == "about" && url.path().eq_ignore_ascii_case("blank"))
}

pub(super) fn required_url<'a>(url: &'a str, tool: &str) -> Result<&'a str, Value> {
    let url = url.trim();
    if url.is_empty() {
        Err(json!({
            "ok": false,
            "code": "ERR_BROWSER_URL_REQUIRED",
            "error": format!("{tool} needs a non-empty url"),
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "executed": false,
        }))
    } else {
        Ok(url)
    }
}

pub(super) fn required_tab_id(tab_id: i64, tool: &str) -> Result<i64, Value> {
    if tab_id > 0 {
        Ok(tab_id)
    } else {
        Err(json!({
            "ok": false,
            "code": "ERR_BROWSER_TAB_ID_REQUIRED",
            "error": format!("{tool} needs a positive tab_id"),
            "target_tab_id": tab_id,
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "executed": false,
        }))
    }
}

pub(super) fn preflight_tab_activation(tab_id: i64, tabs: &Value) -> Result<(), Value> {
    let Some(tabs) = tabs.as_array() else {
        return Err(no_effect_error(
            "ERR_BROWSER_TAB_LIST_INVALID_RESPONSE",
            "browser tab inventory was not an array",
        ));
    };
    if tabs
        .iter()
        .any(|tab| tab.get("id").and_then(Value::as_i64) == Some(tab_id))
    {
        return Ok(());
    }
    let valid_tabs = tabs.iter().filter_map(tab_summary).collect::<Vec<_>>();
    let valid_tabs_total = valid_tabs.len();
    Err(json!({
        "ok": false,
        "code": "ERR_STALE_BROWSER_TARGET",
        "error": format!("browser tab {tab_id} is no longer available"),
        "target_tab_id": tab_id,
        "valid_tabs": valid_tabs.into_iter().take(MAX_VALID_TAB_CHOICES).collect::<Vec<_>>(),
        "valid_tabs_total": valid_tabs_total,
        "valid_tabs_omitted": valid_tabs_total.saturating_sub(MAX_VALID_TAB_CHOICES),
        "retryable": true,
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
    }))
}

pub(super) fn tab_inventory_unavailable(error: &anyhow::Error) -> Value {
    no_effect_error(
        "ERR_BROWSER_TAB_PREFLIGHT_FAILED",
        &format!("could not verify the target tab before activation: {error}"),
    )
}

pub(in crate::overlay::computer_control) fn close_tab_checked(tab_id: i64) -> Value {
    let before = match super::bridge::rpc("tabs", json!({"action": "list"})) {
        Ok(tabs) => tabs,
        Err(error) => return tab_inventory_unavailable(&error),
    };
    if let Err(error) = preflight_tab_activation(tab_id, &before) {
        return error;
    }
    if let Err(error) = super::close_tab(tab_id) {
        let mut result = super::err(error);
        if let Some(object) = result.as_object_mut() {
            object.insert("target_tab_id".to_string(), json!(tab_id));
        }
        return result;
    }

    let deadline = Instant::now() + CLOSE_VERIFY_BUDGET;
    let mut verification_error: Option<String>;
    loop {
        match super::bridge::rpc("tabs", json!({"action": "list"})) {
            Ok(tabs) => match tab_absent(&tabs, tab_id) {
                Some(true) => {
                    return json!({
                        "ok": true,
                        "closed": tab_id,
                        "target_tab_id": tab_id,
                        "effect_verified": true,
                        "effect_may_have_occurred": true,
                        "executed": true,
                    });
                }
                Some(false) => verification_error = None,
                None => {
                    verification_error = Some("browser tab inventory was not an array".to_string())
                }
            },
            Err(error) => verification_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            return json!({
                "ok": false,
                "code": "ERR_BROWSER_TAB_CLOSE_UNVERIFIED",
                "error": verification_error.unwrap_or_else(|| "the browser still reports the tab after close dispatch".to_string()),
                "target_tab_id": tab_id,
                "effect_verified": false,
                "effect_may_have_occurred": true,
            });
        }
        std::thread::sleep(CLOSE_VERIFY_POLL);
    }
}

fn tab_absent(tabs: &Value, tab_id: i64) -> Option<bool> {
    Some(
        !tabs
            .as_array()?
            .iter()
            .any(|tab| tab.get("id").and_then(Value::as_i64) == Some(tab_id)),
    )
}

fn no_effect_error(code: &str, error: &str) -> Value {
    json!({
        "ok": false,
        "code": code,
        "error": error,
        "retryable": true,
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
    })
}

fn tab_summary(tab: &Value) -> Option<Value> {
    let id = tab.get("id").and_then(Value::as_i64).filter(|id| *id > 0)?;
    let (title, url, pending_url) = tab_title_url(tab);
    Some(json!({
        "id": id,
        "title": title,
        "url": url,
        "pending_url": pending_url,
        "active": tab.get("active").and_then(Value::as_bool).unwrap_or(false),
    }))
}

pub(super) fn created_tab_result(requested_url: &str, tab: Value) -> Value {
    let Some(id) = tab.get("id").and_then(Value::as_i64).filter(|id| *id > 0) else {
        return json!({
            "ok": false,
            "code": "ERR_BROWSER_TAB_CREATE_INVALID_RESPONSE",
            "error": "the browser created a tab but omitted its positive id",
            "requested_url": requested_url,
            "effect_may_have_occurred": true,
        });
    };
    json!({
        "ok": true,
        "target_tab_id": id,
        "tab": {
            "id": id,
            "url": nonempty_field(&tab, "url"),
            "pending_url": nonempty_field(&tab, "pendingUrl"),
            "requested_url": requested_url,
        },
        "effect_verified": true,
        "effect_may_have_occurred": true,
        "executed": true,
    })
}

pub(super) fn tab_title_url(tab: &Value) -> (Value, Value, Value) {
    let title = tab.get("title").cloned().unwrap_or(Value::Null);
    let url = nonempty_field(tab, "url").map_or(Value::Null, |url| json!(url));
    let pending_url = nonempty_field(tab, "pendingUrl").map_or(Value::Null, |url| json!(url));
    (title, url, pending_url)
}

fn nonempty_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_tab_keeps_requested_and_pending_urls_separate_from_observation() {
        let pending = created_tab_result(
            "https://requested.invalid/",
            json!({"id": 8, "url": "", "pendingUrl": "https://pending.invalid/"}),
        );
        assert_eq!(pending["tab"]["url"], Value::Null);
        assert_eq!(pending["target_tab_id"], 8);
        assert_eq!(pending["effect_verified"], true);
        assert_eq!(pending["tab"]["pending_url"], "https://pending.invalid/");
        assert_eq!(
            pending["tab"]["requested_url"],
            "https://requested.invalid/"
        );

        let requested =
            created_tab_result("https://requested.invalid/", json!({"id": 9, "url": ""}));
        assert_eq!(requested["tab"]["url"], Value::Null);
        assert_eq!(requested["tab"]["pending_url"], Value::Null);
        assert_eq!(
            requested["tab"]["requested_url"],
            "https://requested.invalid/"
        );
    }

    #[test]
    fn close_verification_requires_a_valid_inventory_without_the_target() {
        assert_eq!(tab_absent(&json!([{"id": 7}, {"id": 9}]), 7), Some(false));
        assert_eq!(tab_absent(&json!([{"id": 9}]), 7), Some(true));
        assert_eq!(tab_absent(&json!({"id": 9}), 7), None);
    }

    #[test]
    fn tab_contract_rejects_missing_required_fields() {
        assert_eq!(
            required_url("  ", "browser_open_tab").unwrap_err()["code"],
            "ERR_BROWSER_URL_REQUIRED"
        );
        assert_eq!(
            required_url("  ", "browser_open_tab").unwrap_err()["effect_may_have_occurred"],
            false
        );
        assert_eq!(
            required_tab_id(0, "browser_switch_tab").unwrap_err()["code"],
            "ERR_BROWSER_TAB_ID_REQUIRED"
        );
        assert_eq!(
            required_tab_id(0, "browser_switch_tab").unwrap_err()["effect_may_have_occurred"],
            false
        );
        assert_eq!(
            created_tab_result("https://requested.invalid/", json!({"url": ""}))["code"],
            "ERR_BROWSER_TAB_CREATE_INVALID_RESPONSE"
        );
    }

    #[test]
    fn activation_preflight_rejects_stale_id_without_dispatch() {
        let error = preflight_tab_activation(
            42,
            &json!([
                {"id": 310, "title": "One", "url": "https://one.invalid/", "active": true},
                {"id": 311, "title": "Two", "pendingUrl": "https://two.invalid/"},
            ]),
        )
        .unwrap_err();
        assert_eq!(error["code"], "ERR_STALE_BROWSER_TARGET");
        assert_eq!(error["effect_may_have_occurred"], false);
        assert_eq!(error["executed"], false);
        assert_eq!(error["valid_tabs"][0]["id"], 310);
        assert_eq!(
            error["valid_tabs"][1]["pending_url"],
            "https://two.invalid/"
        );
    }

    #[test]
    fn activation_preflight_accepts_only_present_positive_id() {
        let tabs = json!([{"id": 310}, {"id": 311}]);
        assert!(preflight_tab_activation(311, &tabs).is_ok());
        let malformed = preflight_tab_activation(311, &json!({})).unwrap_err();
        assert_eq!(malformed["code"], "ERR_BROWSER_TAB_LIST_INVALID_RESPONSE");
        assert_eq!(malformed["effect_may_have_occurred"], false);
    }

    #[test]
    fn tab_listing_keeps_committed_and_pending_urls_separate() {
        let (_, committed, pending) = tab_title_url(&json!({
            "url": "https://committed.invalid/",
            "pendingUrl": "https://pending.invalid/"
        }));
        assert_eq!(committed, "https://committed.invalid/");
        assert_eq!(pending, "https://pending.invalid/");

        let (_, committed, pending) = tab_title_url(&json!({
            "url": "",
            "pendingUrl": "https://pending.invalid/"
        }));
        assert_eq!(committed, Value::Null);
        assert_eq!(pending, "https://pending.invalid/");
    }

    #[test]
    fn document_url_must_be_committed_and_parseable() {
        assert!(bindable_document_url("https://example.invalid/path"));
        assert!(bindable_document_url("data:text/plain,ready"));
        assert!(!bindable_document_url(":"));
        assert!(!bindable_document_url("about:blank"));
    }
}
