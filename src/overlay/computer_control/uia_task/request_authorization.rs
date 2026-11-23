//! Shared request history, independent quorum and decisive-only verdict cache.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use super::super::telemetry::{self, ActionTrace, Privacy};

pub(super) const MAX_REQUEST_TURNS: usize = 6;
const MAX_REQUEST_CHARS: usize = 12_000;
const MIN_POSITIVE_VERDICTS: usize = 2;
const MAX_JSON_CANDIDATES: usize = 64;

pub(super) fn record_request(
    requests: &mut VecDeque<(u64, String)>,
    turn_id: u64,
    text: &str,
) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if let Some((existing_turn, existing_text)) = requests.back_mut()
        && *existing_turn == turn_id
    {
        if existing_text == text {
            return false;
        }
        *existing_text = text.to_string();
    } else {
        requests.push_back((turn_id, text.to_string()));
    }
    while requests.len() > MAX_REQUEST_TURNS
        || requests
            .iter()
            .map(|(_, text)| text.chars().count())
            .sum::<usize>()
            > MAX_REQUEST_CHARS
    {
        requests.pop_front();
    }
    true
}

pub(super) fn begin_turn(requests: &mut VecDeque<(u64, String)>, turn_id: u64, inherit: bool) {
    if !inherit {
        requests.retain(|(request_turn, _)| *request_turn == turn_id);
    }
}

#[derive(Clone)]
pub(super) struct AuthorizationDecision {
    pub(super) authorized: bool,
    pub(super) result: Value,
}

pub(super) fn dispatch_after_check(
    permission: Result<Option<Value>, Value>,
    cancel: &AtomicBool,
    effect: impl FnOnce() -> Value,
) -> Value {
    let approval = match permission {
        Ok(approval) => approval,
        Err(blocked) => return blocked,
    };
    if cancel.load(Ordering::Acquire) {
        return json!({"ok":false,"code":"cancelled","executed":false,"effect_may_have_occurred":false});
    }
    let mut result = effect();
    if let (Some(approval), Some(object)) = (approval, result.as_object_mut()) {
        object.insert("resource_boundary_authorization".to_string(), approval);
    }
    result
}

#[derive(Default)]
pub(super) struct VerdictCache {
    pub(super) entries: HashMap<String, AuthorizationDecision>,
}

pub(super) struct Check<'a> {
    pub(super) instruction: &'a str,
    pub(super) context: &'a str,
    pub(super) settings: Settings,
}

#[derive(Clone, Copy)]
pub(super) struct Settings {
    pub(super) check_timeout: Duration,
    pub(super) provider_timeout: Duration,
    pub(super) artifact_prefix: &'static str,
    pub(super) artifact_kind: &'static str,
    pub(super) event_name: &'static str,
}

pub(super) struct Verdicts {
    pub(super) positive: usize,
    pub(super) negative: usize,
    pub(super) malformed: usize,
    pub(super) reason: String,
}

impl Verdicts {
    fn authorized(&self) -> bool {
        self.positive >= MIN_POSITIVE_VERDICTS && self.negative == 0
    }

    fn decisive(&self) -> bool {
        self.authorized() || self.negative > 0
    }

    pub(super) fn rejection_kind(&self) -> &'static str {
        if self.negative > 0 {
            "request_contract_rejected"
        } else {
            "request_contract_unverified"
        }
    }
}

impl VerdictCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn evaluate(
        &mut self,
        check: Check<'_>,
        cancel: &AtomicBool,
        action: Option<ActionTrace>,
        reject: impl FnOnce(&Verdicts) -> AuthorizationDecision,
    ) -> AuthorizationDecision {
        let exact_input = format!("{}\n\n{}", check.instruction, check.context);
        let input_sha256 = format!("{:x}", Sha256::digest(exact_input.as_bytes()));
        self.evaluate_with(
            &input_sha256,
            || run(check, exact_input, &input_sha256, cancel, action),
            reject,
        )
    }

    fn evaluate_with(
        &mut self,
        identity: &str,
        run: impl FnOnce() -> Verdicts,
        reject: impl FnOnce(&Verdicts) -> AuthorizationDecision,
    ) -> AuthorizationDecision {
        if let Some(cached) = self.entries.get(identity) {
            let mut cached = cached.clone();
            cached.result["cached"] = json!(true);
            return cached;
        }
        let verdicts = run();
        let decision = if verdicts.authorized() {
            AuthorizationDecision {
                authorized: true,
                result: json!({
                    "ok":true, "authorized":true, "positive_verdicts":verdicts.positive,
                    "negative_verdicts":verdicts.negative, "malformed_verdicts":verdicts.malformed,
                    "reason":verdicts.reason,
                }),
            }
        } else {
            reject(&verdicts)
        };
        if verdicts.decisive() {
            self.entries.insert(identity.to_string(), decision.clone());
        }
        decision
    }
}

fn run(
    check: Check<'_>,
    exact_input: String,
    input_sha256: &str,
    cancel: &AtomicBool,
    action: Option<ActionTrace>,
) -> Verdicts {
    let context_sha256 = format!("{:x}", Sha256::digest(check.context.as_bytes()));
    let request_id = telemetry::next_artifact_id();
    let input_artifact = format!(
        "{}-input-{request_id:06}.txt",
        check.settings.artifact_prefix
    );
    let input_write_ok = super::vision::write_artifact(
        check.settings.artifact_kind,
        &input_artifact,
        exact_input.as_bytes(),
        action,
    );
    let instruction = check.instruction.to_string();
    let worker_context = check.context.to_string();
    let settings = check.settings;
    let completed_attempts = Arc::new(Mutex::new(Vec::new()));
    let worker_attempts = Arc::clone(&completed_attempts);
    let report = super::vision::run_cancellable_with_timeout(
        cancel,
        settings.check_timeout,
        move |worker_cancel| {
            let mut positive_seen = 0usize;
            Ok(super::super::vision_reader::read_text_pref_where(
                &instruction,
                &worker_context,
                worker_cancel,
                settings.provider_timeout,
                move |attempt| {
                    if let Ok(mut completed) = worker_attempts.lock() {
                        completed.push(attempt.clone());
                    }
                },
                move |answer| match parse_verdict(answer) {
                    Some((false, _)) => true,
                    Some((true, _)) => {
                        positive_seen += 1;
                        positive_seen >= MIN_POSITIVE_VERDICTS
                    }
                    None => false,
                },
            ))
        },
    );
    let (attempts, worker_error) = match report {
        Ok(report) => (report.attempts, None),
        Err(error) => (
            completed_attempts
                .lock()
                .map(|attempts| attempts.clone())
                .unwrap_or_default(),
            Some(error.to_string()),
        ),
    };
    let (positive, negative, malformed, reason) = classify_answers(
        attempts
            .iter()
            .filter_map(|attempt| attempt.response.as_deref()),
    );
    let verdicts = Verdicts {
        positive,
        negative,
        malformed,
        reason,
    };
    let (candidate_artifact, candidates_write_ok) = super::vision::persist_candidates(
        request_id,
        input_sha256,
        &context_sha256,
        &attempts,
        action,
    );
    let fields = json!({
        "request_id":request_id,"authorized":verdicts.authorized(),
        "positive_verdicts":positive,"negative_verdicts":negative,"malformed_verdicts":malformed,
        "candidate_attempt_count":attempts.len(),"input_sha256":input_sha256,"context_sha256":context_sha256,
        "input_artifact":input_artifact,"input_artifact_write_ok":input_write_ok,
        "candidate_manifest_artifact":candidate_artifact,"candidate_artifacts_write_ok":candidates_write_ok,
        "worker_error":worker_error,
    });
    if let Some(action) = action {
        telemetry::event_for_action(
            settings.event_name,
            "computer_control",
            Privacy::Sensitive,
            action,
            fields,
        );
    } else {
        telemetry::event(
            settings.event_name,
            "computer_control",
            Privacy::Sensitive,
            fields,
        );
    }
    verdicts
}

pub(super) fn classify_answers<'a>(
    answers: impl IntoIterator<Item = &'a str>,
) -> (usize, usize, usize, String) {
    let (mut positive, mut negative, mut malformed) = (0, 0, 0);
    let (mut positive_reason, mut negative_reason) = (None, None);
    for answer in answers {
        match parse_verdict(answer) {
            Some((true, reason)) => {
                positive += 1;
                positive_reason.get_or_insert(reason);
            }
            Some((false, reason)) => {
                negative += 1;
                negative_reason.get_or_insert(reason);
            }
            None => malformed += 1,
        }
    }
    let reason = negative_reason
        .or(positive_reason)
        .unwrap_or_else(|| "no independent model returned a valid authorization verdict".into());
    (positive, negative, malformed, reason)
}

fn parse_verdict(answer: &str) -> Option<(bool, String)> {
    answer
        .match_indices('{')
        .take(MAX_JSON_CANDIDATES)
        .find_map(|(start, _)| {
            let value = serde_json::Deserializer::from_str(&answer[start..])
                .into_iter::<Value>()
                .next()?
                .ok()?;
            let authorized = value.get("authorized")?.as_bool()?;
            let reason = value.get("reason")?.as_str()?.trim();
            (!reason.is_empty()).then(|| (authorized, reason.chars().take(1_024).collect()))
        })
}

#[cfg(test)]
mod tests;
