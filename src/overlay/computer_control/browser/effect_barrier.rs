//! Prevent reconciliation while an admitted browser effect has no terminal receipt.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct Barrier {
    tokens: HashSet<String>,
    synchronizing: bool,
    settled_during_sync: HashSet<String>,
}

impl Barrier {
    fn begin(&mut self, token: &str) -> bool {
        if self.synchronizing || !self.tokens.is_empty() {
            return false;
        }
        self.tokens.insert(token.to_string())
    }

    fn settle(&mut self, token: &str) -> bool {
        if self.synchronizing
            && !self.settled_during_sync.contains(token)
            && self.settled_during_sync.len() >= 256
        {
            return false;
        }
        if self.synchronizing {
            self.settled_during_sync.insert(token.to_string());
        }
        self.tokens.remove(token);
        true
    }
    fn pending(&self) -> bool {
        !self.tokens.is_empty()
    }
    fn ready(&self) -> bool {
        !self.synchronizing && !self.pending()
    }
    /// Adopt the live worker's ledger. Every token still held here was admitted
    /// on an earlier connection (admission is closed while synchronizing). One the
    /// live worker does not report as active cannot still be executing: its
    /// worker either finished it (a terminal replay follows) or died. Such tokens
    /// are retired and returned so they are reported as unverified, never success.
    /// `None` when the live ledger exceeds its bound.
    fn sync(&mut self, tokens: &[String]) -> Option<Vec<String>> {
        let live = tokens
            .iter()
            .filter(|token| !self.settled_during_sync.contains(*token))
            .cloned()
            .collect::<HashSet<_>>();
        if live.len() > 256 {
            return None;
        }
        let mut retired = self.tokens.difference(&live).cloned().collect::<Vec<_>>();
        retired.sort();
        self.tokens = live;
        self.synchronizing = false;
        self.settled_during_sync.clear();
        Some(retired)
    }

    /// A worker without an effect ledger cannot be running an earlier
    /// connection's effects; retire them all and end any unfinished sync.
    fn reset_without_ledger(&mut self) -> Vec<String> {
        let mut retired = self.tokens.drain().collect::<Vec<_>>();
        retired.sort();
        self.synchronizing = false;
        self.settled_during_sync.clear();
        retired
    }
}

fn barrier() -> &'static Mutex<Barrier> {
    static BARRIER: OnceLock<Mutex<Barrier>> = OnceLock::new();
    BARRIER.get_or_init(|| Mutex::new(Barrier::default()))
}

pub(super) fn begin(token: &str) -> bool {
    barrier()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .begin(token)
}

pub(super) fn settle(token: &str) -> bool {
    barrier()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .settle(token)
}

pub(super) fn start_sync() {
    let mut state = barrier().lock().unwrap_or_else(|error| error.into_inner());
    state.synchronizing = true;
}

pub(super) fn sync(tokens: &[String]) -> bool {
    let retired = barrier()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .sync(tokens);
    retired.map(|retired| report_retired(&retired)).is_some()
}

/// Connection setup for a worker that does not advertise effect admission.
pub(super) fn reset_without_ledger() {
    let retired = barrier()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .reset_without_ledger();
    report_retired(&retired);
}

/// The outcome recorded for an effect whose connection ended without terminal
/// evidence: it may have happened, and nothing verified it.
fn retired_outcome(token: &str) -> serde_json::Value {
    serde_json::json!({
        "effect_token": token, "status": "retired_without_terminal",
        "effect_may_have_occurred": true, "effect_verified": false,
    })
}

fn report_retired(retired: &[String]) {
    for token in retired {
        super::super::telemetry::event(
            "browser_effect_retired_unverified",
            "browser_bridge",
            super::super::telemetry::Privacy::Safe,
            retired_outcome(token),
        );
    }
}

pub(super) fn ready() -> bool {
    let state = barrier().lock().unwrap_or_else(|error| error.into_inner());
    state.ready()
}

#[derive(Debug)]
pub(super) struct EffectSettling;

impl std::fmt::Display for EffectSettling {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "the browser extension is synchronizing or a prior browser effect is still settling",
        )
    }
}

impl std::error::Error for EffectSettling {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_matching_terminal_evidence_releases_the_barrier() {
        let mut barrier = Barrier::default();
        assert!(barrier.begin("first"));
        assert!(!barrier.begin("second"));
        assert!(barrier.settle("wrong"));
        assert!(barrier.pending());
        assert!(barrier.settle("first"));
        assert!(!barrier.pending());
        assert!(barrier.begin("second"));
    }

    #[test]
    fn reconnect_sync_keeps_effects_the_live_worker_still_runs() {
        let mut barrier = Barrier::default();
        assert!(barrier.begin("old"));
        barrier.synchronizing = true;
        assert_eq!(
            barrier.sync(&["old".to_string(), "worker".to_string()]),
            Some(vec![])
        );
        assert!(barrier.settle("old"));
        assert!(barrier.pending());
        assert!(barrier.settle("worker"));
        assert!(!barrier.pending());
    }

    #[test]
    fn an_effect_whose_connection_ended_is_retired_as_unverified_and_unblocks() {
        let mut barrier = Barrier::default();
        assert!(barrier.begin("admitted"));
        // The admitting connection ends and a fresh worker connects without it.
        barrier.synchronizing = true;
        assert!(!barrier.ready());
        assert_eq!(barrier.sync(&[]), Some(vec!["admitted".to_string()]));
        let outcome = retired_outcome("admitted");
        assert_eq!(outcome["effect_may_have_occurred"], true);
        assert_eq!(outcome["effect_verified"], false);
        assert!(barrier.ready());
        assert!(barrier.begin("next"), "the next action must be admitted");
        assert!(
            barrier.settle("admitted"),
            "a late replayed terminal is harmless"
        );
        assert!(barrier.pending(), "and cannot release the new effect");
    }

    #[test]
    fn a_worker_without_a_ledger_retires_earlier_effects_and_ends_sync() {
        let mut barrier = Barrier::default();
        assert!(barrier.begin("admitted"));
        barrier.synchronizing = true;
        assert_eq!(barrier.reset_without_ledger(), vec!["admitted".to_string()]);
        assert!(barrier.ready());
        assert!(barrier.begin("next"));
    }

    #[test]
    fn terminal_event_during_sync_cannot_be_resurrected_by_an_older_snapshot() {
        let mut barrier = Barrier {
            synchronizing: true,
            ..Barrier::default()
        };
        assert!(barrier.settle("completed"));
        assert_eq!(barrier.sync(&["completed".to_string()]), Some(vec![]));
        assert!(!barrier.pending());
    }

    #[test]
    fn synchronization_without_known_tokens_still_blocks_effects_and_reconciliation() {
        let mut barrier = Barrier {
            synchronizing: true,
            ..Barrier::default()
        };
        assert!(!barrier.ready());
        assert!(!barrier.begin("next"));
        assert_eq!(barrier.sync(&[]), Some(vec![]));
        assert!(barrier.ready());
        assert!(barrier.begin("next"));
        assert!(!barrier.ready());
        assert!(barrier.settle("next"));
        assert!(barrier.ready());
    }
}
