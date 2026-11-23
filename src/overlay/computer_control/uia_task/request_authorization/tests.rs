use super::*;
use std::cell::Cell;

#[test]
fn denied_or_cancelled_checkpoint_cannot_dispatch_any_alternate_effect() {
    let blocked = json!({"ok":false,"code":"scope_unverified","effect_may_have_occurred":false});
    let result = dispatch_after_check(Err(blocked.clone()), &AtomicBool::new(false), || {
        panic!("denied effect ran")
    });
    assert_eq!(result, blocked);
    for approval in [None, Some(json!({"authorized":true}))] {
        let result = dispatch_after_check(Ok(approval), &AtomicBool::new(true), || {
            panic!("cancelled effect ran")
        });
        assert_eq!(result["code"], "cancelled");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(result["executed"], false);
    }
}

#[test]
fn successful_checkpoint_preserves_effect_uncertainty_and_runs_once() {
    let count = Cell::new(0);
    let result = dispatch_after_check(
        Ok(Some(json!({"authorized":true,"positive_verdicts":2}))),
        &AtomicBool::new(false),
        || {
            count.set(count.get() + 1);
            json!({"ok":false,"effect_may_have_occurred":true,"receipt":"unknown"})
        },
    );
    assert_eq!(count.get(), 1);
    assert_eq!(result["effect_may_have_occurred"], true);
    assert_eq!(result["receipt"], "unknown");
    assert_eq!(
        result["resource_boundary_authorization"]["positive_verdicts"],
        2
    );
}

const POSITIVE: &str = r#"{"authorized":true,"reason":"within request scope"}"#;
const NEGATIVE: &str = r#"{"authorized":false,"reason":"outside request scope"}"#;

fn verdicts(answers: &[&str]) -> Verdicts {
    let (positive, negative, malformed, reason) = classify_answers(answers.iter().copied());
    Verdicts {
        positive,
        negative,
        malformed,
        reason,
    }
}

fn reject(verdicts: &Verdicts) -> AuthorizationDecision {
    AuthorizationDecision {
        authorized: false,
        result: json!({
            "ok":false, "authorization_status":verdicts.rejection_kind(),
            "effect_may_have_occurred":false,
        }),
    }
}

#[test]
fn identical_retry_runs_quorum_again_after_transient_or_insufficient_evidence() {
    for answers in [
        vec![],
        vec!["malformed"],
        vec![POSITIVE],
        vec![POSITIVE, "malformed"],
    ] {
        let mut cache = VerdictCache::default();
        let attempts = Cell::new(0);
        let first = cache.evaluate_with(
            "same-proposal",
            || {
                attempts.set(attempts.get() + 1);
                verdicts(&answers)
            },
            reject,
        );
        assert!(!first.authorized);
        assert_eq!(
            first.result["authorization_status"],
            "request_contract_unverified"
        );
        assert!(cache.entries.is_empty());
        let retry = cache.evaluate_with(
            "same-proposal",
            || {
                attempts.set(attempts.get() + 1);
                verdicts(&[POSITIVE, POSITIVE])
            },
            reject,
        );
        assert!(retry.authorized);
        assert_eq!(attempts.get(), 2);
        assert_eq!(retry.result["positive_verdicts"], 2);
        assert!(retry.result.get("cached").is_none());
    }
}

#[test]
fn only_decisive_scope_verdicts_are_reused_and_clearing_rechecks_them() {
    for answers in [
        vec![POSITIVE, POSITIVE],
        vec![NEGATIVE],
        vec![POSITIVE, POSITIVE, NEGATIVE],
    ] {
        let mut cache = VerdictCache::default();
        let first = cache.evaluate_with("proposal", || verdicts(&answers), reject);
        let cached = cache.evaluate_with("proposal", || panic!("decisive verdict rerun"), reject);
        assert_eq!(cached.authorized, first.authorized);
        assert_eq!(cached.result["cached"], true);
        if answers.contains(&NEGATIVE) {
            assert!(!cached.authorized);
            assert_eq!(
                cached.result["authorization_status"],
                "request_contract_rejected"
            );
        }
        let other = cache.evaluate_with("different-proposal", || verdicts(&[]), reject);
        assert!(!other.authorized);
        assert!(other.result.get("cached").is_none());
        cache.clear();
        let fresh = cache.evaluate_with("proposal", || verdicts(&[]), reject);
        assert!(!fresh.authorized);
        assert!(fresh.result.get("cached").is_none());
    }
}

#[test]
fn verdict_parser_bounds_candidates_and_requires_boolean_and_nonempty_reason() {
    for answer in [
        "",
        "{}",
        r#"{"authorized":"true","reason":"scope"}"#,
        r#"{"authorized":true,"reason":" "}"#,
    ] {
        assert!(parse_verdict(answer).is_none());
    }
    assert_eq!(
        parse_verdict(&format!("prose {POSITIVE}")),
        Some((true, "within request scope".into()))
    );
    let beyond_bound = format!("{}{POSITIVE}", "{} ".repeat(MAX_JSON_CANDIDATES));
    assert!(parse_verdict(&beyond_bound).is_none());
    let long = json!({"authorized":true,"reason":"界".repeat(2_000)}).to_string();
    assert_eq!(parse_verdict(&long).unwrap().1.chars().count(), 1_024);
}

#[test]
fn history_tracks_revisions_and_character_budget_without_interpreting_words() {
    let mut requests = VecDeque::new();
    assert!(record_request(&mut requests, 1, " original "));
    assert!(!record_request(&mut requests, 1, "original"));
    assert!(!record_request(&mut requests, 1, " "));
    assert!(record_request(&mut requests, 1, "revised"));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].1, "revised");
    assert!(record_request(
        &mut requests,
        2,
        &"界".repeat(MAX_REQUEST_CHARS)
    ));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, 2);
    assert!(record_request(
        &mut requests,
        3,
        &"x".repeat(MAX_REQUEST_CHARS + 1)
    ));
    assert!(requests.is_empty());
}
