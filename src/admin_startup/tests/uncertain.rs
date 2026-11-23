use super::*;

fn assert_uncertain(error: anyhow::Error) {
    assert!(
        error
            .downcast_ref::<provider::UncertainElevation>()
            .is_some(),
        "{error:#}"
    );
}

#[test]
fn committed_registration_with_lost_reply_is_disabled_before_normal_rollback_is_allowed() {
    let mut backend = Mock {
        commit_then_fail: true,
        ..Mock::default()
    };
    let error = enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(""),
        SID,
        protected,
    )
    .unwrap_err();
    assert!(
        error
            .downcast_ref::<provider::UncertainElevation>()
            .is_none()
    );
    assert!(!backend.task.unwrap().enabled);
    assert_eq!(
        backend.calls,
        [
            "query", "query", "create", "query", "query", "disable", "query"
        ]
    );
}

#[test]
fn failed_postregistration_readback_is_contained_after_fresh_inspection() {
    let mut backend = Mock {
        fail_query_at: vec![3],
        ..Mock::default()
    };
    let error = enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(""),
        SID,
        protected,
    )
    .unwrap_err();
    assert!(
        error
            .downcast_ref::<provider::UncertainElevation>()
            .is_none()
    );
    assert!(!backend.task.unwrap().enabled);
    assert_eq!(
        backend.calls,
        [
            "query", "query", "create", "query", "query", "query", "disable", "query"
        ]
    );
}

#[test]
fn unavailable_registration_readback_preserves_an_uncertain_outcome() {
    for committed_reply_failed in [false, true] {
        let mut backend = Mock {
            commit_then_fail: committed_reply_failed,
            fail_query_at: if committed_reply_failed {
                vec![3]
            } else {
                vec![3, 4]
            },
            ..Mock::default()
        };
        assert_uncertain(
            enable_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                protected,
            )
            .unwrap_err(),
        );
        assert!(backend.task.unwrap().enabled);
        assert!(!backend.calls.contains(&"disable"));
    }
}

#[test]
fn failed_containment_never_turns_an_active_registration_into_a_known_failure() {
    for lost_reply in [false, true] {
        let mut backend = Mock {
            commit_then_fail: lost_reply,
            bad_descriptor: !lost_reply,
            fail_disable: true,
            ..Mock::default()
        };
        assert_uncertain(
            enable_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                protected,
            )
            .unwrap_err(),
        );
        assert!(backend.task.unwrap().enabled);
        assert!(backend.calls.contains(&"disable"));
    }
}

#[test]
fn containment_requires_readback_of_the_disabled_registration() {
    let mut backend = Mock {
        bad_descriptor: true,
        fail_query_at: vec![5],
        ..Mock::default()
    };
    assert_uncertain(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected,
        )
        .unwrap_err(),
    );
    assert!(!backend.task.unwrap().enabled);
}

#[test]
fn failed_update_of_an_unprotected_prior_target_retires_it_instead_of_restoring_elevation() {
    let mut backend = Mock {
        task: Some(snapshot(PRIOR)),
        fail_register: true,
        ..Mock::default()
    };
    let error = enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(PRIOR),
        SID,
        |path| {
            if path == Path::new(CURRENT) {
                protected(path)
            } else {
                unsafe_target(path)
            }
        },
    )
    .unwrap_err();
    assert!(
        error
            .downcast_ref::<provider::UncertainElevation>()
            .is_none()
    );
    assert!(!backend.task.unwrap().enabled);
    assert_eq!(
        backend.calls,
        [
            "query", "query", "update", "query", "query", "disable", "query"
        ]
    );
}

#[test]
fn a_native_mutation_uncertainty_marker_survives_absence_or_unchanged_state() {
    for existing in [None, Some(snapshot(PRIOR))] {
        let mut backend = Mock {
            task: existing.clone(),
            fail_register: true,
            sticky_register_failure: true,
            ..Mock::default()
        };
        assert_uncertain(
            enable_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(PRIOR),
                SID,
                protected,
            )
            .unwrap_err(),
        );
        assert_eq!(backend.task, existing);
        assert!(!backend.calls.contains(&"disable"));
    }
}

#[test]
fn observed_containment_does_not_claim_a_failed_native_rpc_has_quiesced() {
    let mut backend = Mock {
        commit_then_fail: true,
        sticky_register_failure: true,
        ..Mock::default()
    };
    assert_uncertain(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected,
        )
        .unwrap_err(),
    );
    assert!(!backend.task.unwrap().enabled);
    assert!(backend.calls.contains(&"disable"));
}

#[test]
fn a_local_preflight_rejection_does_not_require_followup_rpc_to_prove_non_submission() {
    let mut backend = Mock {
        preflight_error: Some(Issue::PathTooLong),
        fail_query_at: vec![3],
        ..Mock::default()
    };
    let error = enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(""),
        SID,
        protected,
    )
    .unwrap_err();
    assert!(error.downcast_ref::<NotSubmitted>().is_some());
    assert_eq!(error.downcast_ref::<Issue>(), Some(&Issue::PathTooLong));
    assert!(
        error
            .downcast_ref::<provider::UncertainElevation>()
            .is_none()
    );
    assert!(backend.task.is_none());
    assert_eq!(backend.query_count, 2);
}
