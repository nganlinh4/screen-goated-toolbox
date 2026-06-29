//! Bounded MCP setup task guard. It does not know app-specific recipes; it only
//! tracks whether setup is making progress and forces status/probe based exits.

use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct SetupGuard {
    active: Option<ActiveSetup>,
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

impl SetupGuard {
    pub(super) fn before_action(&self, name: &str) -> Option<Value> {
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

        let Some(active) = self.active.as_mut() else {
            return;
        };
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

    pub(super) fn after_ground(&mut self, notes: &[(&'static str, &'static str)]) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let no_change = notes
            .iter()
            .any(|(k, _)| matches!(*k, "screen_change" | "ui_change" | "stuck_warning"));
        if no_change {
            active.no_change_strikes += 1;
        } else {
            active.no_change_strikes = 0;
        }
    }

    pub(super) fn note(&self) -> Option<(&'static str, &'static str)> {
        self.active.as_ref().map(|_| {
            (
                "setup_guard",
                "MCP integration setup is active. Verify with app_integration_status; success is the readiness probe plus active MCP tools, not a visual guess. If GUI actions stop changing the screen, switch to scripting/CLI or stop with the blocker.",
            )
        })
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
