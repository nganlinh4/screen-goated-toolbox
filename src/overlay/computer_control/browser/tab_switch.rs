//! `browser_switch_tab`: activate an exact tab and bring its window forward.
//!
//! A switch is what a user does when they click a tab: the tab becomes active
//! and its browser window comes to the front, so native pointer and keyboard
//! input reach it. Chrome may be refused the foreground by Windows; the receipt
//! then reports the observed focus rather than claiming it.

use serde_json::{Value, json};

use super::{bridge, capabilities, err, tab_lifecycle};

const UNFOCUSED_NOTE: &str = "The tab is active in its window, but another window stayed in front. Native mouse and keyboard input will not reach this tab until its browser window is focused.";

pub(in crate::overlay::computer_control) fn switch_tab(tab_id: i64) -> Value {
    let tab_id = match tab_lifecycle::required_tab_id(tab_id, "browser_switch_tab") {
        Ok(tab_id) => tab_id,
        Err(error) => return error,
    };
    if let Some(error) = super::conn_guard() {
        return error;
    }
    let tabs = match bridge::rpc("tabs", json!({"action": "list"})) {
        Ok(tabs) => tabs,
        Err(error) => return tab_lifecycle::tab_inventory_unavailable(&error),
    };
    if let Err(error) = tab_lifecycle::preflight_tab_activation(tab_id, &tabs) {
        return error;
    }
    let request = activation_request(tab_id);
    let activation = match bridge::rpc("tabs", request.clone()) {
        Ok(activation) => activation,
        Err(error) => return err(error),
    };
    // Confirm WHERE we landed (title + url) so the agent doesn't keep reading/acting on the wrong tab.
    let (title, url, pending_url) = tab_title_url(tab_id);
    let mut receipt = json!({
        "ok": true,
        "switched": tab_id,
        "target_tab_id": tab_id,
        "title": title,
        "url": url,
        "pending_url": pending_url,
        "effect_verified": true,
        "effect_may_have_occurred": true,
        "executed": true,
    });
    add_window_focus(&mut receipt, &request, &activation);
    receipt
}

/// Ask for window focus only from an extension that advertises it; an older
/// extension activates the tab exactly as before.
fn activation_request(tab_id: i64) -> Value {
    let mut request = json!({"action": "activate", "tabId": tab_id});
    if capabilities::supports(capabilities::TABS_ACTIVATE_FOCUS_WINDOW) {
        request["focusWindow"] = json!(true);
    }
    request
}

fn add_window_focus(receipt: &mut Value, request: &Value, activation: &Value) {
    let window_focused = if request.get("focusWindow").and_then(Value::as_bool) == Some(true) {
        activation
            .get("windowFocused")
            .and_then(Value::as_bool)
            .map_or(Value::Null, Value::Bool)
    } else {
        Value::Null
    };
    if window_focused == Value::Bool(false) {
        receipt["window_focus_note"] = json!(UNFOCUSED_NOTE);
    }
    receipt["window_focused"] = window_focused;
}

/// A tab's title, committed URL, and pending URL (best-effort; nulls on miss).
fn tab_title_url(tab_id: i64) -> (Value, Value, Value) {
    if let Ok(v) = bridge::rpc("tabs", json!({"action": "list"}))
        && let Some(t) = v.as_array().and_then(|tabs| {
            tabs.iter()
                .find(|t| t.get("id").and_then(Value::as_i64) == Some(tab_id))
        })
    {
        return tab_lifecycle::tab_title_url(t);
    }
    (Value::Null, Value::Null, Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_switch_asks_for_window_focus_only_when_the_extension_supports_it() {
        {
            let _scope = capabilities::test_scope::advertise(&[
                capabilities::TABS_ACTIVATE,
                capabilities::TABS_ACTIVATE_FOCUS_WINDOW,
            ]);
            assert_eq!(
                activation_request(7),
                json!({"action": "activate", "tabId": 7, "focusWindow": true})
            );
        }
        let _scope = capabilities::test_scope::advertise(&[capabilities::TABS_ACTIVATE]);
        assert_eq!(
            activation_request(7),
            json!({"action": "activate", "tabId": 7})
        );
    }

    #[test]
    fn the_receipt_reports_observed_window_focus_without_claiming_it() {
        let focus = json!({"action": "activate", "tabId": 7, "focusWindow": true});
        let mut focused = json!({"ok": true});
        add_window_focus(&mut focused, &focus, &json!({"windowFocused": true}));
        assert_eq!(focused["window_focused"], true);
        assert!(focused.get("window_focus_note").is_none());

        let mut refused = json!({"ok": true});
        add_window_focus(&mut refused, &focus, &json!({"windowFocused": false}));
        assert_eq!(refused["window_focused"], false);
        assert_eq!(refused["window_focus_note"], UNFOCUSED_NOTE);

        let mut unknown = json!({"ok": true});
        add_window_focus(&mut unknown, &focus, &json!({}));
        assert!(unknown["window_focused"].is_null());

        let mut legacy = json!({"ok": true});
        let plain = json!({"action": "activate", "tabId": 7});
        add_window_focus(&mut legacy, &plain, &json!({"windowFocused": true}));
        assert!(legacy["window_focused"].is_null());
    }
}
