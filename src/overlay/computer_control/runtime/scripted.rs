//! Production scripted-run completion contract.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::super::telemetry::{self, Privacy};
use super::reader::Reader;
#[cfg(any(debug_assertions, test))]
use super::scripted_snapshots::ScriptedSnapshots;

#[cfg(not(any(debug_assertions, test)))]
const SNAPSHOT_ENV_NAMES: [&str; 2] = [
    "CC_SCRIPTED_SNAPSHOT_PATHS_JSON",
    "CC_SCRIPTED_SNAPSHOT_DIR",
];

pub(super) fn idle_settle() -> Duration {
    Duration::from_secs(env_seconds("CC_SCRIPTED_IDLE_SETTLE_SECS", 2))
}

pub(super) fn deadline() -> Duration {
    Duration::from_secs(env_seconds("CC_SCRIPTED_DEADLINE_SECS", 300))
}

pub(super) enum ScriptedStep {
    Wait,
    Inject(String),
    Complete,
}

pub(super) fn runtime_idle(state: &Reader, playback_active: bool, transport_debt: bool) -> bool {
    state.pending.id.is_none()
        && state.immediate_tool_responses.is_empty()
        && state.turn_cleanup_pending.is_none()
        && !state.reconciliation_required
        && !state.awaiting
        && !state.active
        && !playback_active
        && !transport_debt
}

/// Owns the scripted-run turn queue and its completion contract. Each finished
/// turn is judged before the next is injected. A missing, rejected, partial, or
/// duplicate final response is recorded as typed evidence and the pass continues
/// for coverage, but the run still fails at the end.
pub(super) struct ScriptedDriver {
    turns: VecDeque<String>,
    started: Instant,
    idle_since: Option<Instant>,
    unverified_turns: u32,
    idle_settle: Duration,
    injected_turns: usize,
    finalized_turns: usize,
    completion_emitted: bool,
    #[cfg(any(debug_assertions, test))]
    snapshots: Option<ScriptedSnapshots>,
}

impl ScriptedDriver {
    pub(super) fn new(turns: Vec<String>) -> anyhow::Result<Self> {
        #[cfg(any(debug_assertions, test))]
        let snapshots = ScriptedSnapshots::from_environment()?;
        #[cfg(not(any(debug_assertions, test)))]
        reject_release_snapshot_configuration()?;

        Ok(Self {
            turns: turns.into(),
            started: Instant::now(),
            idle_since: None,
            unverified_turns: 0,
            idle_settle: idle_settle(),
            injected_turns: 0,
            finalized_turns: 0,
            completion_emitted: false,
            #[cfg(any(debug_assertions, test))]
            snapshots,
        })
    }

    pub(super) fn step(&mut self, state: &Reader, idle: bool) -> anyhow::Result<ScriptedStep> {
        self.step_at(state, idle, Instant::now(), deadline())
    }

    fn step_at(
        &mut self,
        state: &Reader,
        idle: bool,
        now: Instant,
        run_deadline: Duration,
    ) -> anyhow::Result<ScriptedStep> {
        if now.saturating_duration_since(self.started) > run_deadline {
            anyhow::bail!(
                "scripted production run exceeded {}s",
                run_deadline.as_secs()
            );
        }
        if !idle {
            self.idle_since = None;
            return Ok(ScriptedStep::Wait);
        }
        if self.injected_turns == 0 && !self.turns.is_empty() {
            return Ok(self.inject_next());
        }
        let idle_since = self.idle_since.get_or_insert(now);
        if now.saturating_duration_since(*idle_since) < self.idle_settle {
            return Ok(ScriptedStep::Wait);
        }
        let settled_idle = now.saturating_duration_since(*idle_since);
        self.finish_previous_turn(state, settled_idle)?;
        self.idle_since = None;
        if !self.turns.is_empty() {
            return Ok(self.inject_next());
        }
        if self.unverified_turns > 0 {
            anyhow::bail!(
                "{} scripted turn(s) became idle without exactly one accepted delivered final response",
                self.unverified_turns
            );
        }
        if !self.completion_emitted {
            telemetry::event(
                "scripted_run_idle_complete",
                "test_harness",
                Privacy::Safe,
                serde_json::json!({
                    "turn_count": self.finalized_turns,
                    "unverified_turns": self.unverified_turns,
                    "idle_settle_ms": self.idle_settle.as_millis(),
                    "settled_idle_ms": settled_idle.as_millis(),
                }),
            );
            self.completion_emitted = true;
        }
        Ok(ScriptedStep::Complete)
    }

    fn inject_next(&mut self) -> ScriptedStep {
        let command = self.turns.pop_front().expect("queue checked as non-empty");
        telemetry::event(
            "scripted_turn_injected",
            "test_harness",
            Privacy::UserText,
            serde_json::json!({
                "scripted_turn_index": self.injected_turns + 1,
                "remaining_turns": self.turns.len(),
                "command_preview": command.chars().take(240).collect::<String>(),
            }),
        );
        self.injected_turns += 1;
        self.idle_since = None;
        ScriptedStep::Inject(command)
    }

    fn finish_previous_turn(
        &mut self,
        state: &Reader,
        settled_idle: Duration,
    ) -> anyhow::Result<()> {
        if self.injected_turns == self.finalized_turns {
            return Ok(());
        }
        anyhow::ensure!(
            self.injected_turns == self.finalized_turns + 1,
            "scripted completion state skipped a turn boundary"
        );
        #[cfg(any(debug_assertions, test))]
        {
            if let Some(snapshots) = &self.snapshots {
                snapshots.capture_turn(self.injected_turns)?;
            }
        }
        self.finalized_turns = self.injected_turns;
        let accepted = turn_outcome_acceptable(state);
        self.record_turn_outcome(state);
        telemetry::event(
            "scripted_turn_finalized",
            "test_harness",
            Privacy::Safe,
            serde_json::json!({
                "scripted_turn_index": self.finalized_turns,
                "accepted": accepted,
                "terminal_accepted": state.terminal_accepted,
                "final_response_delivered": state.terminal_final_response_delivered,
                "dropped_events": state.terminal_dropped_events,
                "effectful_dropped_events": state.terminal_effectful_dropped_events,
                "idle_settle_ms": self.idle_settle.as_millis(),
                "settled_idle_ms": settled_idle.as_millis(),
            }),
        );
        Ok(())
    }

    fn record_turn_outcome(&mut self, state: &Reader) {
        if turn_outcome_acceptable(state) {
            return;
        }
        self.unverified_turns += 1;
        telemetry::typed_error(
            "ERR_SCRIPTED_TURN_UNVERIFIED",
            "test_harness",
            "scripted turn became idle without exactly one accepted delivered final response",
            serde_json::json!({"tools": state.turn_tools.clone()}),
        );
    }
}

#[cfg(not(any(debug_assertions, test)))]
fn reject_release_snapshot_configuration() -> anyhow::Result<()> {
    if SNAPSHOT_ENV_NAMES
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        anyhow::bail!("scripted file snapshots are available only in debug or test builds");
    }
    Ok(())
}

fn env_seconds(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

pub(super) fn has_accepted_completion(state: &Reader) -> bool {
    state.terminal_drain
        && state.terminal_accepted
        && state.turn_summary_emitted
        && state.terminal_final_response_delivered
        && state.terminal_effectful_dropped_events == 0
}

/// Per-turn lifecycle contract: every scripted request owes exactly one
/// accepted, structurally completed final response. Task correctness is checked
/// by the independent run oracle, not by this runtime latch.
pub(super) fn turn_outcome_acceptable(state: &Reader) -> bool {
    has_accepted_completion(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sgt-scripted-driver-{}-{suffix}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let temp = std::env::temp_dir();
            assert_eq!(self.0.parent(), Some(temp.as_path()));
            assert!(
                self.0
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("sgt-scripted-driver-"))
            );
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn driver_with_snapshots(turns: &[&str], source: PathBuf, root: PathBuf) -> ScriptedDriver {
        ScriptedDriver {
            turns: turns.iter().map(|turn| (*turn).to_string()).collect(),
            started: Instant::now(),
            idle_since: None,
            unverified_turns: 0,
            idle_settle: Duration::ZERO,
            injected_turns: 0,
            finalized_turns: 0,
            completion_emitted: false,
            snapshots: Some(ScriptedSnapshots::for_test(vec![source], root).unwrap()),
        }
    }

    fn driver_with_idle_settle(turns: &[&str], idle_settle: Duration) -> ScriptedDriver {
        ScriptedDriver {
            turns: turns.iter().map(|turn| (*turn).to_string()).collect(),
            started: Instant::now(),
            idle_since: None,
            unverified_turns: 0,
            idle_settle,
            injected_turns: 0,
            finalized_turns: 0,
            completion_emitted: false,
            snapshots: None,
        }
    }

    fn accepted_state() -> Reader {
        Reader {
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            terminal_final_response_delivered: true,
            ..Reader::default()
        }
    }

    #[test]
    fn non_idle_observation_clears_the_current_idle_interval() {
        let settle = Duration::from_secs(60);
        let mut driver = driver_with_idle_settle(&["alpha"], settle);
        let state = accepted_state();
        let base = Instant::now();
        driver.started = base;

        assert!(matches!(
            driver
                .step_at(&state, true, base, Duration::MAX)
                .unwrap(),
            ScriptedStep::Inject(command) if command == "alpha"
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + Duration::from_secs(1), Duration::MAX,)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(driver.idle_since.is_some());

        assert!(matches!(
            driver
                .step_at(&state, false, base + settle, Duration::MAX)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(driver.idle_since.is_none());
    }

    #[test]
    fn completion_requires_a_fresh_full_idle_interval_after_busy_work() {
        let settle = Duration::from_secs(60);
        let mut driver = driver_with_idle_settle(&["alpha"], settle);
        let state = accepted_state();
        let base = Instant::now();
        driver.started = base;

        assert!(matches!(
            driver
                .step_at(&state, true, base, Duration::MAX)
                .unwrap(),
            ScriptedStep::Inject(command) if command == "alpha"
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + Duration::from_secs(1), Duration::MAX,)
                .unwrap(),
            ScriptedStep::Wait
        ));

        assert!(matches!(
            driver
                .step_at(&state, false, base + settle, Duration::MAX)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + settle, Duration::MAX)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(matches!(
            driver
                .step_at(
                    &state,
                    true,
                    base + settle + settle - Duration::from_nanos(1),
                    Duration::MAX,
                )
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + settle + settle, Duration::MAX,)
                .unwrap(),
            ScriptedStep::Complete
        ));
    }

    #[test]
    fn next_turn_waits_for_a_fresh_full_idle_interval_after_busy_work() {
        let settle = Duration::from_secs(60);
        let mut driver = driver_with_idle_settle(&["alpha", "beta"], settle);
        let state = accepted_state();
        let base = Instant::now();
        driver.started = base;

        assert!(matches!(
            driver
                .step_at(&state, true, base, Duration::MAX)
                .unwrap(),
            ScriptedStep::Inject(command) if command == "alpha"
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + Duration::from_secs(1), Duration::MAX,)
                .unwrap(),
            ScriptedStep::Wait
        ));

        assert!(matches!(
            driver
                .step_at(&state, false, base + settle, Duration::MAX)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(matches!(
            driver
                .step_at(&state, true, base + settle, Duration::MAX)
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert!(matches!(
            driver
                .step_at(
                    &state,
                    true,
                    base + settle + settle - Duration::from_nanos(1),
                    Duration::MAX,
                )
                .unwrap(),
            ScriptedStep::Wait
        ));
        assert_eq!(driver.injected_turns, 1);
        assert_eq!(driver.finalized_turns, 0);

        assert!(matches!(
            driver
                .step_at(
                    &state,
                    true,
                    base + settle + settle,
                    Duration::MAX,
                )
                .unwrap(),
            ScriptedStep::Inject(command) if command == "beta"
        ));
        assert_eq!(driver.injected_turns, 2);
        assert_eq!(driver.finalized_turns, 1);
    }

    #[test]
    fn snapshots_each_completed_turn_before_the_next_transition() {
        let temp = TestDir::new();
        let source = temp.0.join("state.bin");
        let root = temp.0.join("snapshots");
        std::fs::write(&source, b"initial").unwrap();
        let mut driver = driver_with_snapshots(&["alpha", "beta"], source.clone(), root.clone());
        let state = Reader {
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            terminal_final_response_delivered: true,
            ..Reader::default()
        };

        assert!(matches!(
            driver.step(&state, true).unwrap(),
            ScriptedStep::Inject(command) if command == "alpha"
        ));
        assert!(!root.join("turn-0001").exists());

        std::fs::write(&source, b"after first").unwrap();
        assert!(matches!(
            driver.step(&state, true).unwrap(),
            ScriptedStep::Inject(command) if command == "beta"
        ));
        assert_eq!(
            std::fs::read(root.join("turn-0001/file-0001.snapshot")).unwrap(),
            b"after first"
        );

        std::fs::write(&source, b"after final").unwrap();
        assert!(matches!(
            driver.step(&state, true).unwrap(),
            ScriptedStep::Complete
        ));
        assert_eq!(
            std::fs::read(root.join("turn-0002/file-0001.snapshot")).unwrap(),
            b"after final"
        );
    }

    #[test]
    fn every_turn_requires_one_accepted_delivered_final_response() {
        let spoke_only = Reader::default();
        assert!(!turn_outcome_acceptable(&spoke_only));

        let observed = Reader {
            turn_tools: vec!["observe".into(), "browser_read_page".into()],
            ..Reader::default()
        };
        assert!(!turn_outcome_acceptable(&observed));

        let observed_and_answered = Reader {
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            terminal_final_response_delivered: true,
            ..observed
        };
        assert!(turn_outcome_acceptable(&observed_and_answered));

        let mutated_without_completion = Reader {
            turn_tools: vec!["observe".into(), "act".into()],
            ..Reader::default()
        };
        assert!(!turn_outcome_acceptable(&mutated_without_completion));

        let unknown_tool = Reader {
            turn_tools: vec!["future_capability".into()],
            ..Reader::default()
        };
        assert!(!turn_outcome_acceptable(&unknown_tool));

        let mutated_and_accepted = Reader {
            turn_tools: vec!["act".into()],
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            terminal_final_response_delivered: true,
            ..Reader::default()
        };
        assert!(turn_outcome_acceptable(&mutated_and_accepted));
    }

    #[test]
    fn idle_is_not_success_without_acceptance_and_a_delivered_final_response() {
        let idle = Reader {
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(!has_accepted_completion(&idle));

        let accepted_without_output = Reader {
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(!has_accepted_completion(&accepted_without_output));

        let accepted = Reader {
            terminal_final_response_delivered: true,
            ..accepted_without_output
        };
        assert!(has_accepted_completion(&accepted));

        let accepted_with_late_effect = Reader {
            terminal_effectful_dropped_events: 1,
            ..accepted
        };
        assert!(!has_accepted_completion(&accepted_with_late_effect));

        let failed = Reader {
            terminal_drain: true,
            terminal_accepted: false,
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(!has_accepted_completion(&failed));
    }

    #[test]
    fn runtime_idle_waits_for_queued_responses_and_reconciliation() {
        let mut state = Reader::default();
        assert!(runtime_idle(&state, false, false));
        state.immediate_tool_responses.push_back((
            "call".into(),
            "tool".into(),
            serde_json::json!({"ok": false}),
        ));
        assert!(!runtime_idle(&state, false, false));
        state.immediate_tool_responses.clear();
        state.turn_cleanup_pending = Some(9);
        assert!(!runtime_idle(&state, false, false));
        state.turn_cleanup_pending = None;
        state.reconciliation_required = true;
        assert!(!runtime_idle(&state, false, false));
        state.reconciliation_required = false;
        assert!(!runtime_idle(&state, false, true));
    }
}
