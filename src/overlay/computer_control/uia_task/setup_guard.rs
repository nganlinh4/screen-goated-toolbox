//! Turn-scoped retry bounds for setup-provider operations.
//!
//! This guard never owns unrelated capabilities. Generic GUI/action convergence
//! is handled by the normal postcondition machinery; setup state only limits
//! repeated setup, status, reset, and documentation calls for the same provider.

use serde_json::{Value, json};

const MAX_STATUS_CHECKS: u32 = 4;
const MAX_DOC_READS: u32 = 1;
const MAX_RESET_ATTEMPTS: u32 = 1;

#[derive(Default)]
pub(super) struct SetupGuard {
    turn_id: Option<u64>,
    integration: Option<IntegrationSetup>,
    browser: Option<BrowserSetup>,
}

struct IntegrationSetup {
    id: String,
    setup_id: String,
    status_checks: u32,
    docs_reads: u32,
}

#[derive(Default)]
struct BrowserSetup {
    setup_attempts: u32,
    status_checks: u32,
    reset_attempts: u32,
}

impl SetupGuard {
    pub(super) fn begin_turn(&mut self, turn_id: u64) {
        if self.turn_id != Some(turn_id) {
            self.retire();
            self.turn_id = Some(turn_id);
        }
    }

    pub(super) fn retire(&mut self) {
        self.integration = None;
        self.browser = None;
    }

    pub(super) fn before_action(&self, name: &str) -> Option<Value> {
        self.before_integration_action(name)
            .or_else(|| self.before_browser_action(name))
    }

    fn before_integration_action(&self, name: &str) -> Option<Value> {
        let setup = self.integration.as_ref()?;
        let exhausted = match name {
            "setup_app_integration" => Some("setup_already_started"),
            "app_integration_status" if setup.status_checks >= MAX_STATUS_CHECKS => {
                Some("status_limit_reached")
            }
            "read_app_integration_docs" if setup.docs_reads >= MAX_DOC_READS => {
                Some("documentation_already_read")
            }
            _ => None,
        }?;
        Some(json!({
            "ok": false,
            "code": "ERR_SETUP_RETRY_LIMIT",
            "blocked_operation": name,
            "reason": exhausted,
            "id": setup.id,
            "setup_id": setup.setup_id,
            "terminal_blocker": true,
            "instruction": "Stop repeating setup operations for this turn and report the latest typed readiness state once."
        }))
    }

    fn before_browser_action(&self, name: &str) -> Option<Value> {
        let setup = self.browser.as_ref()?;
        let exhausted = match name {
            "browser_setup" if setup.setup_attempts >= 1 => Some("setup_already_started"),
            "browser_status" if setup.status_checks >= MAX_STATUS_CHECKS => {
                Some("status_limit_reached")
            }
            "browser_reset" if setup.reset_attempts >= MAX_RESET_ATTEMPTS => {
                Some("reset_limit_reached")
            }
            _ => None,
        }?;
        Some(json!({
            "ok": false,
            "code": "ERR_BROWSER_SETUP_RETRY_LIMIT",
            "blocked_operation": name,
            "reason": exhausted,
            "terminal_blocker": true,
            "instruction": "Stop repeating browser setup operations for this turn and report the latest typed connection state once."
        }))
    }

    pub(super) fn record_result(&mut self, name: &str, result: &Value) {
        if name == "setup_app_integration"
            && result.get("ok").and_then(Value::as_bool).unwrap_or(false)
            && let Some(id) = result.get("id").and_then(Value::as_str)
            && let Some(setup_id) = result.get("setup_id").and_then(Value::as_str)
        {
            self.integration = Some(IntegrationSetup {
                id: id.to_string(),
                setup_id: setup_id.to_string(),
                status_checks: 0,
                docs_reads: 0,
            });
        }

        if name == "browser_setup" && result.get("ok").and_then(Value::as_bool) == Some(true) {
            if result.get("connected").and_then(Value::as_bool) == Some(true) {
                self.browser = None;
            } else {
                self.browser = Some(BrowserSetup {
                    setup_attempts: 1,
                    ..BrowserSetup::default()
                });
            }
        }

        if let Some(setup) = self.integration.as_mut() {
            match name {
                "app_integration_status" => {
                    setup.status_checks = setup.status_checks.saturating_add(1);
                    if result.get("ready").and_then(Value::as_bool) == Some(true) {
                        self.integration = None;
                    }
                }
                "read_app_integration_docs" => {
                    setup.docs_reads = setup.docs_reads.saturating_add(1);
                }
                _ => {}
            }
        }

        if let Some(setup) = self.browser.as_mut() {
            match name {
                "browser_status" => {
                    setup.status_checks = setup.status_checks.saturating_add(1);
                    if result.get("connected").and_then(Value::as_bool) == Some(true) {
                        self.browser = None;
                    }
                }
                "browser_reset" => {
                    setup.reset_attempts = setup.reset_attempts.saturating_add(1);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disconnected_browser() -> Value {
        json!({"ok": true, "connected": false, "state": "reconnecting"})
    }

    #[test]
    fn setup_state_is_retired_on_new_turn_and_terminal_boundary() {
        let mut guard = SetupGuard::default();
        guard.begin_turn(7);
        guard.record_result("browser_setup", &disconnected_browser());
        assert!(guard.browser.is_some());

        guard.begin_turn(7);
        assert!(guard.browser.is_some());
        guard.begin_turn(8);
        assert!(guard.browser.is_none());

        guard.record_result(
            "setup_app_integration",
            &json!({"ok": true, "id": "provider", "setup_id": "setup"}),
        );
        assert!(guard.integration.is_some());
        guard.retire();
        assert!(guard.integration.is_none());
    }

    #[test]
    fn browser_setup_limits_only_setup_operations() {
        let mut guard = SetupGuard::default();
        guard.begin_turn(3);
        guard.record_result("browser_setup", &disconnected_browser());

        assert!(guard.before_action("browser_setup").is_some());
        assert!(guard.before_action("run_command").is_none());
        assert!(guard.before_action("observe").is_none());

        for _ in 0..MAX_STATUS_CHECKS {
            assert!(guard.before_action("browser_status").is_none());
            guard.record_result("browser_status", &disconnected_browser());
        }
        let blocked = guard.before_action("browser_status").unwrap();
        assert_eq!(blocked["reason"], "status_limit_reached");
        assert_eq!(blocked["terminal_blocker"], true);
        assert!(guard.before_action("list_files").is_none());
    }

    #[test]
    fn integration_retry_limits_do_not_capture_other_capabilities() {
        let mut guard = SetupGuard::default();
        guard.begin_turn(9);
        guard.record_result(
            "setup_app_integration",
            &json!({"ok": true, "id": "provider", "setup_id": "setup"}),
        );

        guard.record_result("read_app_integration_docs", &json!({"ok": true}));
        assert!(guard.before_action("read_app_integration_docs").is_some());
        assert!(guard.before_action("browser_tabs").is_none());
        assert!(guard.before_action("future_provider_tool").is_none());
    }
}
