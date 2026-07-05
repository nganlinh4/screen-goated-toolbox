//! Bounded MCP setup task guard. It does not know app-specific recipes; it only
//! tracks whether setup is making progress and forces status/probe based exits.

use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct SetupGuard {
    active: Option<ActiveSetup>,
    browser: Option<BrowserSetup>,
}

struct ActiveSetup {
    id: String,
    setup_id: String,
    actions: u32,
    gui_actions: u32,
    no_change_strikes: u32,
    status_checks: u32,
    activation_pending: bool,
}

struct BrowserSetup {
    actions: u32,
    gui_actions: u32,
    no_change_strikes: u32,
    status_checks: u32,
}

impl SetupGuard {
    pub(super) fn before_action(&self, name: &str) -> Option<Value> {
        if let Some(blocked) = self.before_mcp_action(name) {
            return Some(blocked);
        }
        self.before_browser_action(name)
    }

    fn before_mcp_action(&self, name: &str) -> Option<Value> {
        let active = self.active.as_ref()?;
        if matches!(
            name,
            "app_integration_status" | "read_app_integration_docs" | "done"
        ) {
            return None;
        }
        if active.activation_pending {
            return Some(blocked(
                active,
                "integration tools need session activation",
                "Stop setup actions. The integration health check passed, but the Live session must reconnect before its MCP tools can be used.",
            ));
        }
        if active.actions >= 24 {
            return Some(blocked(
                active,
                "setup action budget exhausted",
                "Call app_integration_status now. If it is not ready, stop and tell the user the concrete blocker instead of continuing.",
            ));
        }
        if active.no_change_strikes >= 3 || active.gui_actions >= 10 {
            return Some(blocked(
                active,
                "setup is not converging through GUI actions",
                "Switch to a programmatic/scripting/CLI route from read_app_integration_docs, or call app_integration_status and stop with the blocker.",
            ));
        }
        None
    }

    fn before_browser_action(&self, name: &str) -> Option<Value> {
        let browser = self.browser.as_ref()?;
        if matches!(
            name,
            "browser_status" | "browser_reset" | "browser_setup" | "done"
        ) {
            return None;
        }
        if browser.actions >= 18 {
            return Some(browser_blocked(
                "setup action budget exhausted",
                "Call browser_status now. If connected is false, stop and explain the concrete blocker instead of continuing.",
            ));
        }
        if browser.no_change_strikes >= 2 || browser.gui_actions >= 8 {
            return Some(browser_blocked(
                "browser setup is not converging through GUI actions",
                "Call browser_status now. If connected is false, change route once or stop with the blocker; do not keep reopening chrome://extensions.",
            ));
        }
        if browser.status_checks >= 4 {
            return Some(browser_blocked(
                "browser status did not become connected",
                "Stop setup and report the blocker. More polling will not fix a disabled, blocked, or unloaded extension.",
            ));
        }
        None
    }

    pub(super) fn record_result(&mut self, name: &str, result: &Value) {
        if name == "setup_app_integration"
            && result.get("ok").and_then(Value::as_bool).unwrap_or(false)
            && let Some(id) = result.get("id").and_then(Value::as_str)
            && let Some(setup_id) = result.get("setup_id").and_then(Value::as_str)
        {
            self.active = Some(ActiveSetup {
                id: id.to_string(),
                setup_id: setup_id.to_string(),
                actions: 0,
                gui_actions: 0,
                no_change_strikes: 0,
                status_checks: 0,
                activation_pending: false,
            });
            return;
        }
        if name == "browser_setup" && result.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            if result
                .get("connected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                self.browser = None;
            } else {
                self.browser = Some(BrowserSetup {
                    actions: 0,
                    gui_actions: 0,
                    no_change_strikes: 0,
                    status_checks: 0,
                });
            }
        }
        if name == "browser_status"
            && result
                .get("connected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            self.browser = None;
        }

        if let Some(active) = self.active.as_mut() {
            active.actions += 1;
            if is_gui_action(name) {
                active.gui_actions += 1;
            }
            if name == "app_integration_status" {
                active.status_checks += 1;
                active.activation_pending = result
                    .get("activation_pending")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if result
                    .get("ready")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    self.active = None;
                }
            }
        }
        if let Some(browser) = self.browser.as_mut() {
            browser.actions += 1;
            if is_gui_action(name) {
                browser.gui_actions += 1;
            }
            if name == "browser_status" {
                browser.status_checks += 1;
            }
        }
    }

    pub(super) fn after_ground(&mut self, notes: &[(&'static str, &'static str)]) {
        let no_change = notes
            .iter()
            .any(|(k, _)| matches!(*k, "screen_change" | "ui_change" | "stuck_warning"));
        if let Some(active) = self.active.as_mut() {
            if no_change {
                active.no_change_strikes += 1;
            } else {
                active.no_change_strikes = 0;
            }
        }
        if let Some(browser) = self.browser.as_mut() {
            if no_change {
                browser.no_change_strikes += 1;
            } else {
                browser.no_change_strikes = 0;
            }
        }
    }

    pub(super) fn note(&self) -> Option<(&'static str, &'static str)> {
        if self.active.is_some() {
            return Some((
                "setup_guard",
                "MCP integration setup is active. Verify with app_integration_status; success is the readiness probe plus active MCP tools, not a visual guess. If GUI actions stop changing the screen, switch to scripting/CLI or stop with the blocker.",
            ));
        }
        if self.browser.is_some() {
            return Some((
                "setup_guard",
                "Browser-control setup is active. Success is browser_status connected:true, not a visual guess. After each visible setup step, check browser_status; if actions stop changing the screen, stop with the blocker.",
            ));
        }
        None
    }
}

fn blocked(active: &ActiveSetup, reason: &str, instruction: &str) -> Value {
    json!({
        "ok": false,
        "blocked": reason,
        "setup_id": active.setup_id,
        "id": active.id,
        "instruction": instruction,
        "status_tool": "app_integration_status",
        "docs_tool": "read_app_integration_docs",
    })
}

fn browser_blocked(reason: &str, instruction: &str) -> Value {
    json!({
        "ok": false,
        "blocked": reason,
        "code": "ERR_BROWSER_SETUP_NOT_CONVERGING",
        "instruction": instruction,
        "status_tool": "browser_status",
        "reset_tool": "browser_reset",
    })
}

fn is_gui_action(name: &str) -> bool {
    matches!(
        name,
        "click_at"
            | "click_target"
            | "click_mark"
            | "drag"
            | "drag_target"
            | "zoom"
            | "scroll"
            | "type_text"
            | "key_combination"
            | "act"
            | "do_steps"
    )
}
