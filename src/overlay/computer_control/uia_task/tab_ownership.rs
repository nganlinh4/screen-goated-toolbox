//! Turn-scoped ownership and verified retirement for model-opened browser tabs.

use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::super::browser::{TemporaryBrowserTab, TemporaryTabCleanup};

const RETIREMENT_BUDGET: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabLifetime {
    Turn,
    Persistent,
}

impl Drop for super::Brain {
    fn drop(&mut self) {
        self.retire_owned_tabs(RetirementReason::SessionEnd);
    }
}

impl TabLifetime {
    pub(super) fn parse(args: &Value) -> Result<Self, Value> {
        match args.get("lifetime") {
            None => Ok(Self::Persistent),
            Some(value) => Self::parse_value(value, "browser_open_tab"),
        }
    }

    pub(super) fn parse_required(args: &Value, tool: &str) -> Result<Self, Value> {
        match args.get("lifetime") {
            Some(value) => Self::parse_value(value, tool),
            None => Err(json!({
                "ok": false,
                "code": "ERR_BROWSER_TAB_LIFETIME_REQUIRED",
                "error": format!("{tool} needs an explicit turn or persistent lifetime"),
                "retryable": true,
                "effect_verified": false,
                "effect_may_have_occurred": false,
                "executed": false,
            })),
        }
    }

    fn parse_value(value: &Value, tool: &str) -> Result<Self, Value> {
        match value.as_str() {
            Some("turn") => Ok(Self::Turn),
            Some("persistent") => Ok(Self::Persistent),
            _ => Err(json!({
                "ok": false,
                "code": "ERR_BROWSER_TAB_LIFETIME_INVALID",
                "error": format!("{tool} lifetime must be turn or persistent"),
                "retryable": true,
                "effect_verified": false,
                "effect_may_have_occurred": false,
                "executed": false,
            })),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::Persistent => "persistent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RetirementReason {
    Completed,
    Interrupted,
    Superseded,
    SessionEnd,
}

impl RetirementReason {
    fn preserve_active(self) -> bool {
        !matches!(self, Self::Completed)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Superseded => "superseded",
            Self::SessionEnd => "session_end",
        }
    }
}

#[derive(Default)]
pub(super) struct TurnTabOwnership {
    tabs: Vec<TemporaryBrowserTab>,
}

impl TurnTabOwnership {
    pub(super) fn track(&mut self, tab: TemporaryBrowserTab) {
        if self.owns(tab.id) {
            return;
        }
        self.tabs.push(tab);
    }

    pub(super) fn owns(&self, tab_id: i64) -> bool {
        self.tabs.iter().any(|tab| tab.id == tab_id)
    }

    /// Convert an exact turn lease into a persistent tab without closing it.
    pub(super) fn promote(&mut self, tab_id: i64) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        self.tabs.remove(index);
        true
    }

    pub(super) fn retire(
        &mut self,
        turn_id: Option<u64>,
        reason: RetirementReason,
    ) -> RetirementReport {
        let deadline = Instant::now() + RETIREMENT_BUDGET;
        let mut report = RetirementReport {
            turn_id,
            reason,
            attempted: self.tabs.len(),
            ..RetirementReport::default()
        };
        for tab in std::mem::take(&mut self.tabs).into_iter().rev() {
            let cleanup = super::super::browser::close_tab_verified_until(
                &tab,
                deadline,
                reason.preserve_active(),
            );
            report.record(tab.id, cleanup);
        }
        report
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.tabs.len()
    }
}

pub(super) struct RetirementReport {
    turn_id: Option<u64>,
    reason: RetirementReason,
    attempted: usize,
    closed_verified: usize,
    preserved: usize,
    failed: usize,
    restore_failed: usize,
    details: Vec<Value>,
}

impl Default for RetirementReport {
    fn default() -> Self {
        Self {
            turn_id: None,
            reason: RetirementReason::SessionEnd,
            attempted: 0,
            closed_verified: 0,
            preserved: 0,
            failed: 0,
            restore_failed: 0,
            details: Vec::new(),
        }
    }
}

impl RetirementReport {
    fn record(&mut self, tab_id: i64, cleanup: TemporaryTabCleanup) {
        if cleanup.closed_verified {
            self.closed_verified += 1;
        } else if cleanup.preserved {
            self.preserved += 1;
        } else {
            self.failed += 1;
        }
        if cleanup.restore_error.is_some() {
            self.restore_failed += 1;
        }
        self.details.push(json!({
            "tab_id": tab_id,
            "closed_verified": cleanup.closed_verified,
            "preserved": cleanup.preserved,
            "preservation_reason": cleanup.preservation_reason,
            "restoration_required": cleanup.restoration_required,
            "restored": cleanup.restored,
            "close_error": cleanup.close_error,
            "restore_error": cleanup.restore_error,
        }));
    }

    pub(super) fn record_telemetry(self) {
        if self.attempted == 0 {
            return;
        }
        let payload = json!({
            "turn_id": self.turn_id,
            "reason": self.reason.as_str(),
            "attempted": self.attempted,
            "closed_verified": self.closed_verified,
            "preserved": self.preserved,
            "failed": self.failed,
            "restore_failed": self.restore_failed,
            "details": self.details,
        });
        super::super::telemetry::event(
            "browser_turn_tabs_retired",
            "browser_lifecycle",
            super::super::telemetry::Privacy::Safe,
            payload.clone(),
        );
        if self.failed > 0 {
            super::super::telemetry::typed_error(
                "ERR_BROWSER_TURN_TAB_RETIREMENT_INCOMPLETE",
                "browser_lifecycle",
                "one or more turn-owned browser tabs could not be verified closed",
                payload,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup(closed: bool, preserved: bool) -> TemporaryTabCleanup {
        TemporaryTabCleanup {
            closed_verified: closed,
            preserved,
            preservation_reason: preserved.then(|| "active_user_takeover".to_string()),
            restoration_required: false,
            restored: false,
            close_error: (!closed && !preserved).then(|| "transport unavailable".to_string()),
            restore_error: None,
        }
    }

    #[test]
    fn omitted_new_tab_lifetime_preserves_the_user_visible_tab() {
        assert_eq!(
            TabLifetime::parse(&json!({})).unwrap(),
            TabLifetime::Persistent
        );
        assert_eq!(
            TabLifetime::parse(&json!({"lifetime": "persistent"})).unwrap(),
            TabLifetime::Persistent
        );
        let invalid = TabLifetime::parse(&json!({"lifetime": "session"})).unwrap_err();
        assert_eq!(invalid["code"], "ERR_BROWSER_TAB_LIFETIME_INVALID");
        assert_eq!(invalid["effect_may_have_occurred"], false);
    }

    #[test]
    fn navigation_lifetime_is_required_before_any_effect() {
        let missing = TabLifetime::parse_required(&json!({}), "browser_navigate").unwrap_err();
        assert_eq!(missing["code"], "ERR_BROWSER_TAB_LIFETIME_REQUIRED");
        assert_eq!(missing["effect_may_have_occurred"], false);
        assert_eq!(missing["executed"], false);
        assert_eq!(
            TabLifetime::parse_required(&json!({"lifetime": "turn"}), "browser_navigate").unwrap(),
            TabLifetime::Turn
        );
    }

    #[test]
    fn promotion_removes_only_the_exact_owned_lease() {
        let mut ownership = TurnTabOwnership::default();
        ownership.track(TemporaryBrowserTab::test_lease(71));
        ownership.track(TemporaryBrowserTab::test_lease(72));
        ownership.track(TemporaryBrowserTab::test_lease(72));
        assert!(ownership.owns(71));
        assert!(ownership.owns(72));
        assert_eq!(ownership.len(), 2);

        assert!(ownership.promote(71));
        assert!(!ownership.owns(71));
        assert!(ownership.owns(72));
        assert_eq!(ownership.len(), 1);
        assert!(!ownership.promote(71));
    }

    #[test]
    fn retirement_distinguishes_verified_close_takeover_and_failure() {
        let mut report = RetirementReport {
            attempted: 3,
            ..RetirementReport::default()
        };
        report.record(1, cleanup(true, false));
        report.record(2, cleanup(false, true));
        report.record(3, cleanup(false, false));
        assert_eq!(report.closed_verified, 1);
        assert_eq!(report.preserved, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.details.len(), 3);
    }

    #[test]
    fn only_normal_completion_may_retire_an_active_foreground_lease() {
        assert!(!RetirementReason::Completed.preserve_active());
        assert!(RetirementReason::Interrupted.preserve_active());
        assert!(RetirementReason::Superseded.preserve_active());
        assert!(RetirementReason::SessionEnd.preserve_active());
        assert_eq!(TurnTabOwnership::default().len(), 0);
    }
}
