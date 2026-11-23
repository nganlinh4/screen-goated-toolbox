pub(super) use crate::api::gemini_live::response_deadlines::ResponsePolicy;
use crate::api::gemini_live::response_deadlines::{DeadlineOutcome, ResponseDeadlines};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaitOutcome {
    Pending,
    Complete,
    FirstOutputTimeout,
    HardTimeout,
    Closed,
}

pub(super) struct ResponseProgress {
    deadlines: ResponseDeadlines,
    idle_ms: Option<u64>,
    content_started: bool,
    require_interaction_idle: bool,
    terminal: Option<WaitOutcome>,
}

impl ResponseProgress {
    pub fn new(policy: ResponsePolicy, require_interaction_idle: bool, started_ms: u64) -> Self {
        Self {
            deadlines: ResponseDeadlines::new(
                started_ms,
                Some(policy.first_output_ms),
                require_interaction_idle.then_some(policy.hard_response_ms),
            ),
            idle_ms: (!require_interaction_idle).then_some(policy.content_idle_ms),
            content_started: false,
            require_interaction_idle,
            terminal: None,
        }
    }

    pub fn content_started(&self) -> bool {
        self.content_started
    }

    pub fn observe_text(&mut self, now_ms: u64, text: &str) {
        if !text.is_empty() {
            self.content_started = true;
            self.deadlines.observe_output(now_ms, self.idle_ms);
        }
    }

    pub fn outcome(&mut self, now_ms: u64, complete: bool, closed: bool) -> WaitOutcome {
        if let Some(outcome) = self.terminal {
            return outcome;
        }
        if closed && !(self.content_started() && !self.require_interaction_idle) {
            self.terminal = Some(WaitOutcome::Closed);
            return WaitOutcome::Closed;
        }
        let outcome = match self.deadlines.outcome(now_ms, complete || closed) {
            DeadlineOutcome::Pending => WaitOutcome::Pending,
            DeadlineOutcome::Complete => WaitOutcome::Complete,
            DeadlineOutcome::FirstOutputTimeout => WaitOutcome::FirstOutputTimeout,
            DeadlineOutcome::HardTimeout => WaitOutcome::HardTimeout,
        };
        if outcome != WaitOutcome::Pending {
            self.terminal = Some(outcome);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_live_wait_timeline_cases_execute_response_deadline_policy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/preset-system/live-response-wait.json"
        ))
        .unwrap();
        let policy = ResponsePolicy {
            first_output_ms: fixture["firstOutputTimeoutMillis"].as_u64().unwrap(),
            content_idle_ms: fixture["idleCompletionMillis"].as_u64().unwrap(),
            hard_response_ms: fixture["hardCompletionMillis"].as_u64().unwrap(),
        };
        assert_eq!(ResponsePolicy::default(), policy);
        for case in fixture["cases"].as_array().unwrap() {
            let mut state =
                ResponseProgress::new(policy, case["interactionIdle"].as_bool().unwrap(), 0);
            for event in case["events"].as_array().unwrap() {
                let now = event["atMs"].as_u64().unwrap();
                let kind = event["kind"].as_str().unwrap();
                if kind == "text" {
                    state.observe_text(now, event["text"].as_str().unwrap());
                }
                let outcome = state.outcome(now, kind == "complete", kind == "closed");
                let expected = match event["expected"].as_str().unwrap() {
                    "pending" => WaitOutcome::Pending,
                    "complete" => WaitOutcome::Complete,
                    "first_output_timeout" => WaitOutcome::FirstOutputTimeout,
                    "hard_timeout" => WaitOutcome::HardTimeout,
                    "closed" => WaitOutcome::Closed,
                    other => panic!("unknown expected outcome {other}"),
                };
                assert_eq!(outcome, expected, "{} at {now}", case["name"]);
            }
            assert_eq!(
                state.content_started(),
                case["firstOutput"].as_bool().unwrap()
            );
        }
    }
}
