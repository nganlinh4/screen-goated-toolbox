//! Independent semantic checkpoint for exact CSV/TSV structural mutations.
//!
//! A content-bound preflight token proves proposal identity, not user intent.
//! Before the private commit edge, a separate text-model quorum compares the
//! exact proposal with bounded user-authored request history. Rust never grants
//! permission from language-specific keywords.

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[cfg(test)]
use super::request_authorization::MAX_REQUEST_TURNS;
#[cfg(test)]
use super::request_authorization::classify_answers;
use super::request_authorization::{self, AuthorizationDecision, Check, Settings, VerdictCache};

const MAX_PROPOSAL_CHARS: usize = 16_000;
const CHECK_TIMEOUT: Duration = Duration::from_secs(28);
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(9);

#[derive(Default)]
pub(super) struct StructuralAuthorization {
    requests: VecDeque<(u64, String)>,
    cached: VerdictCache,
}

impl StructuralAuthorization {
    pub(super) fn record_request(&mut self, turn_id: u64, text: &str) {
        if request_authorization::record_request(&mut self.requests, turn_id, text) {
            self.cached.clear();
        }
    }

    pub(super) fn begin_turn(&mut self, turn_id: u64, inherit: bool) {
        request_authorization::begin_turn(&mut self.requests, turn_id, inherit);
        self.cached.clear();
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
        self.cached.evaluate(
            Check {
                instruction,
                context: &context,
                settings: Settings {
                    check_timeout: CHECK_TIMEOUT,
                    provider_timeout: PROVIDER_TIMEOUT,
                    artifact_prefix: "structure-authorization",
                    artifact_kind: "structure_authorization_input",
                    event_name: "structure_authorization_verdict",
                },
            },
            cancel,
            action,
            |verdicts| {
                rejected(
                    verdicts.rejection_kind(),
                    verdicts.reason.clone(),
                    verdicts.positive,
                    verdicts.negative,
                    verdicts.malformed,
                )
            },
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revised_same_turn_request_retires_cached_structural_approval() {
        let mut authorization = StructuralAuthorization::default();
        authorization.record_request(7, "Update the selected table.");
        authorization.cached.entries.insert(
            "old-context".into(),
            AuthorizationDecision {
                authorized: true,
                result: json!({"authorized":true}),
            },
        );
        authorization.record_request(7, "Update the selected table.");
        assert_eq!(authorization.cached.entries.len(), 1);
        authorization.record_request(7, "Preserve the table shape and formulas.");
        assert!(authorization.cached.entries.is_empty());
        assert_eq!(authorization.requests.len(), 1);
        assert_eq!(
            authorization.requests[0].1,
            "Preserve the table shape and formulas."
        );
    }

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
