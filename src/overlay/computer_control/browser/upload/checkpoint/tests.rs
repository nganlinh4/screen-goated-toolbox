use super::*;
use serde_json::json;
use std::cell::Cell;

#[test]
fn denied_or_unverified_disclosure_never_revalidates_or_dispatches() {
    for status in ["rejected", "unverified"] {
        let blocked =
            json!({"ok":false,"authorization_status":status,"effect_may_have_occurred":false});
        let result = before_disclosure::<()>(
            &json!({}),
            &AtomicBool::new(false),
            |_| Err(blocked.clone()),
            || panic!("blocked proposal reached target validation"),
            |_| panic!("blocked proposal reached file assignment"),
        );
        assert_eq!(result, blocked);
    }
}

#[test]
fn cancellation_at_each_checkpoint_stops_disclosure() {
    for phase in 0..3 {
        let cancel = AtomicBool::new(phase == 0);
        let authorized = Cell::new(false);
        let validated = Cell::new(false);
        let result = before_disclosure(
            &json!({}),
            &cancel,
            |_| {
                authorized.set(true);
                if phase == 1 {
                    cancel.store(true, Ordering::Release);
                }
                Ok(json!({"authorized":true}))
            },
            || {
                validated.set(true);
                cancel.store(true, Ordering::Release);
                Ok(())
            },
            |_| panic!("cancelled proposal reached file assignment"),
        );
        assert_eq!(authorized.get(), phase != 0);
        assert_eq!(validated.get(), phase == 2);
        assert_eq!(result["code"], "ERR_BROWSER_UPLOAD_CANCELLED");
        assert_eq!(result["executed"], false);
        assert_eq!(result["effect_may_have_occurred"], false);
    }
}

#[test]
fn changed_source_or_receiver_never_reaches_assignment() {
    let stale = json!({"ok":false,"stale":true,"effect_may_have_occurred":false});
    let result = before_disclosure::<()>(
        &json!({}),
        &AtomicBool::new(false),
        |_| Ok(json!({"authorized":true})),
        || Err(stale.clone()),
        |_| panic!("stale proposal reached file assignment"),
    );
    assert_eq!(result, stale);
}

#[test]
fn approved_exact_proposal_dispatches_only_the_revalidated_target() {
    let proposal = json!({"source":{"file_identity":27},"receiver":{"document_id":"doc"}});
    let result = before_disclosure(
        &proposal,
        &AtomicBool::new(false),
        |observed| {
            assert_eq!(observed, &proposal);
            Ok(json!({"authorized":true,"positive_verdicts":2}))
        },
        || Ok(41),
        |target| {
            assert_eq!(target, 41);
            json!({"ok":false,"effect_may_have_occurred":true,"receipt":"unknown"})
        },
    );
    assert_eq!(
        result["resource_scope_authorization"]["positive_verdicts"],
        2
    );
    assert_eq!(result["effect_may_have_occurred"], true);
    assert_eq!(result["receipt"], "unknown");
}
