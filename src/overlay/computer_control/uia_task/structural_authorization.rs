//! Independent semantic checkpoint for exact CSV/TSV structural mutations.
//!
//! A content-bound preflight token proves proposal identity, not user intent.
//! Before the private commit edge, a separate text-model quorum compares the
//! exact proposal with bounded user-authored request history. Rust never grants
//! permission from language-specific keywords.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use std::time::Duration;

const MAX_REQUEST_TURNS: usize = 6;
const MAX_REQUEST_CHARS: usize = 12_000;
const MAX_PROPOSAL_CHARS: usize = 16_000;
const CHECK_TIMEOUT: Duration = Duration::from_secs(28);
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(9);
const MIN_POSITIVE_VERDICTS: usize = 2;
const MAX_JSON_CANDIDATES: usize = 64;

#[derive(Default)]
pub(super) struct StructuralAuthorization {
    requests: VecDeque<(u64, String)>,
    cached: Option<(String, AuthorizationDecision)>,
}

#[derive(Clone)]
pub(super) struct AuthorizationDecision {
    pub(super) authorized: bool,
    pub(super) result: Value,
}

impl StructuralAuthorization {
    pub(super) fn record_request(&mut self, turn_id: u64, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self
            .requests
            .back()
            .is_some_and(|(existing_turn, _)| *existing_turn == turn_id)
        {
            return;
        }
        self.requests.push_back((turn_id, text.to_string()));
        self.cached = None;
        while self.requests.len() > MAX_REQUEST_TURNS || self.request_chars() > MAX_REQUEST_CHARS {
            self.requests.pop_front();
        }
    }

    pub(super) fn begin_turn(&mut self, turn_id: u64, inherit: bool) {
        if !inherit {
            self.requests
                .retain(|(request_turn, _)| *request_turn == turn_id);
        }
        self.cached = None;
    }

    pub(super) fn evaluate(
        &mut self,
        args: &Value,
        preflight: &Value,
        cancel: &AtomicBool,
        action: Option<super::super::telemetry::ActionTrace>,
    ) -> AuthorizationDecision {
        let context = match self.context(args, preflight) {
            Ok(context) => context,
            Err(reason) => return rejected("proposal_not_assessable", reason, 0, 0, 0),
        };
        let instruction = "Act as an independent request-contract checker. Decide whether the user-authored request history explicitly authorizes the exact proposed CSV/TSV record-shape or formula-cell change. Ordinary data updates do not imply permission to remove headers, change row/column shape, or rewrite formulas. Earlier user constraints remain binding unless a later user request clearly changes them. Authorize only when the structural effect itself is directly requested and not contradicted. Return one JSON object only: {\"authorized\":boolean,\"reason\":\"brief exact reason\"}.";
        let exact_input = format!("{instruction}\n\n{context}");
        let input_sha256 = format!("{:x}", Sha256::digest(exact_input.as_bytes()));
        let context_sha256 = format!("{:x}", Sha256::digest(context.as_bytes()));
        if let Some((cached_hash, cached)) = &self.cached
            && cached_hash == &context_sha256
        {
            let mut cached = cached.clone();
            cached.result["cached"] = json!(true);
            return cached;
        }
        let request_id = super::super::telemetry::next_artifact_id();
        let input_artifact = format!("structure-authorization-input-{request_id:06}.txt");
        let input_write_ok = super::vision::write_artifact(
            "structure_authorization_input",
            &input_artifact,
            exact_input.as_bytes(),
            action,
        );
        let instruction = instruction.to_string();
        let worker_context = context.clone();
        let completed_attempts = Arc::new(Mutex::new(Vec::new()));
        let worker_attempts = Arc::clone(&completed_attempts);
        let report = super::vision::run_cancellable_with_timeout(
            cancel,
            CHECK_TIMEOUT,
            move |worker_cancel| {
                let mut positive_seen = 0usize;
                Ok(super::super::vision_reader::read_text_pref_where(
                    &instruction,
                    &worker_context,
                    worker_cancel,
                    PROVIDER_TIMEOUT,
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
        let (positive, negative, malformed, reason) = classify(&attempts);
        let (candidate_artifact, candidates_write_ok) = super::vision::persist_candidates(
            request_id,
            &input_sha256,
            &context_sha256,
            &attempts,
            action,
        );
        let authorized = positive >= MIN_POSITIVE_VERDICTS && negative == 0;
        let result = if authorized {
            json!({
                "ok": true,
                "authorized": true,
                "positive_verdicts": positive,
                "negative_verdicts": negative,
                "malformed_verdicts": malformed,
                "reason": reason,
            })
        } else {
            rejected(
                if negative > 0 {
                    "request_contract_rejected"
                } else {
                    "request_contract_unverified"
                },
                reason,
                positive,
                negative,
                malformed,
            )
            .result
        };
        record_event(
            action,
            json!({
                "request_id": request_id,
                "authorized": authorized,
                "positive_verdicts": positive,
                "negative_verdicts": negative,
                "malformed_verdicts": malformed,
                "candidate_attempt_count": attempts.len(),
                "input_sha256": input_sha256,
                "context_sha256": context_sha256,
                "input_artifact": input_artifact,
                "input_artifact_write_ok": input_write_ok,
                "candidate_manifest_artifact": candidate_artifact,
                "candidate_artifacts_write_ok": candidates_write_ok,
                "worker_error": worker_error,
            }),
        );
        let decision = AuthorizationDecision { authorized, result };
        self.cached = Some((context_sha256, decision.clone()));
        decision
    }

    fn context(&self, args: &Value, preflight: &Value) -> Result<String, String> {
        if self.requests.is_empty() {
            return Err("no committed user request history is available".to_string());
        }
        let replacements = args
            .get("replacements")
            .and_then(Value::as_array)
            .ok_or_else(|| "the proposal has no replacement list".to_string())?;
        let proposal_chars = replacements.iter().try_fold(0usize, |total, replacement| {
            let old = replacement
                .get("old_text")
                .and_then(Value::as_str)
                .ok_or_else(|| "a replacement has no old_text".to_string())?;
            let new = replacement
                .get("new_text")
                .and_then(Value::as_str)
                .ok_or_else(|| "a replacement has no new_text".to_string())?;
            total
                .checked_add(old.chars().count())
                .and_then(|value| value.checked_add(new.chars().count()))
                .filter(|value| *value <= MAX_PROPOSAL_CHARS)
                .ok_or_else(|| {
                    "the structural proposal is too large to assess atomically; split it into narrower exact changes"
                        .to_string()
                })
        })?;
        let history = self
            .requests
            .iter()
            .map(|(turn_id, text)| json!({"turn_id": turn_id, "text": text}))
            .collect::<Vec<_>>();
        let changes = replacements
            .iter()
            .enumerate()
            .map(|(index, replacement)| {
                json!({
                    "index": index,
                    "expected_count": replacement.get("expected_count"),
                    "old_text": replacement.get("old_text"),
                    "new_text": replacement.get("new_text"),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&json!({
            "user_request_history": history,
            "proposed_structural_change": {
                "path": args.get("path"),
                "replacement_text_chars": proposal_chars,
                "replacements": changes,
                "preflight": {
                    "code": preflight.get("code"),
                    "format": preflight.get("format"),
                    "before_record_count": preflight.get("before_record_count"),
                    "after_record_count": preflight.get("after_record_count"),
                    "before_formula_count": preflight.get("before_formula_count"),
                    "after_formula_count": preflight.get("after_formula_count"),
                    "before_field_counts": preflight.get("before_field_counts"),
                    "after_field_counts": preflight.get("after_field_counts"),
                    "parse_error": preflight.get("parse_error"),
                },
            },
        }))
        .map_err(|error| format!("could not serialize the structural proposal: {error}"))
    }

    fn request_chars(&self) -> usize {
        self.requests
            .iter()
            .map(|(_, text)| text.chars().count())
            .sum()
    }
}

fn classify(
    attempts: &[super::super::vision_reader::CandidateAttempt],
) -> (usize, usize, usize, String) {
    classify_answers(
        attempts
            .iter()
            .filter_map(|attempt| attempt.response.as_deref()),
    )
}

fn classify_answers<'a>(
    answers: impl IntoIterator<Item = &'a str>,
) -> (usize, usize, usize, String) {
    let mut positive = 0;
    let mut negative = 0;
    let mut malformed = 0;
    let mut negative_reason = None;
    let mut positive_reason = None;
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

fn rejected(
    kind: &str,
    reason: String,
    positive: usize,
    negative: usize,
    malformed: usize,
) -> AuthorizationDecision {
    let code = if kind == "request_contract_rejected" {
        "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED"
    } else {
        "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED"
    };
    AuthorizationDecision {
        authorized: false,
        result: json!({
            "ok": false,
            "code": code,
            "error": reason,
            "authorization_status": kind,
            "positive_verdicts": positive,
            "negative_verdicts": negative,
            "malformed_verdicts": malformed,
            "effect_may_have_occurred": false,
            "effect_verified": false,
            "executed": false,
            "original_unchanged": true,
            "instruction": "The content-bound token identifies this proposal but cannot authorize it. Keep the file unchanged, use a structure-preserving edit, or ask the user to explicitly request the exact row, column, or formula change.",
        }),
    }
}

fn record_event(action: Option<super::super::telemetry::ActionTrace>, fields: Value) {
    use super::super::telemetry::{self, Privacy};
    if let Some(action) = action {
        telemetry::event_for_action(
            "structure_authorization_verdict",
            "computer_control",
            Privacy::Sensitive,
            action,
            fields,
        );
    } else {
        telemetry::event(
            "structure_authorization_verdict",
            "computer_control",
            Privacy::Sensitive,
            fields,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_needs_independent_positive_quorum_and_no_negative() {
        let positive = r#"{"authorized":true,"reason":"explicitly requested"}"#;
        let negative = r#"{"authorized":false,"reason":"preservation was required"}"#;
        assert_eq!(classify_answers([positive, positive]).0, 2);
        let (_, negatives, _, reason) = classify_answers([positive, positive, negative]);
        assert_eq!(negatives, 1);
        assert_eq!(reason, "preservation was required");
    }

    #[test]
    fn request_history_is_turn_deduplicated_and_bounded() {
        let mut authorization = StructuralAuthorization::default();
        authorization.record_request(1, "first");
        authorization.record_request(1, "duplicate same turn");
        for turn in 2..=9 {
            authorization.record_request(turn, &format!("turn {turn}"));
        }
        assert_eq!(authorization.requests.len(), MAX_REQUEST_TURNS);
        assert_eq!(authorization.requests.back().unwrap().0, 9);
    }

    #[test]
    fn unrelated_turn_drops_prior_structural_scope() {
        let mut authorization = StructuralAuthorization::default();
        authorization.record_request(1, "first scope");
        authorization.record_request(2, "second scope");
        authorization.begin_turn(2, false);
        assert_eq!(authorization.requests.len(), 1);
        assert_eq!(authorization.requests.front().unwrap().0, 2);
    }

    #[test]
    fn inherited_turn_keeps_prior_structural_scope() {
        let mut authorization = StructuralAuthorization::default();
        authorization.record_request(1, "first scope");
        authorization.record_request(2, "correction");
        authorization.begin_turn(2, true);
        assert_eq!(authorization.requests.len(), 2);
    }
}
