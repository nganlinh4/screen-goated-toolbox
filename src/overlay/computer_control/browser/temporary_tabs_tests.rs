use super::*;

fn tab(id: i64, window: i64, active: bool, url: &str) -> TabSnapshot {
    TabSnapshot {
        id,
        window_id: Some(window),
        active,
        url: Some(url.into()),
        pending_url: None,
    }
}

#[test]
fn sole_delta_is_not_ownership_without_returned_id_or_url_proof() {
    let before = [tab(1, 10, true, "https://before.test")];
    let after = [before[0].clone(), tab(2, 10, false, "https://other.test")];
    assert!(select_created_tab(&before, &after, None, "https://wanted.test/", true).is_none());
    assert!(select_created_tab(&before, &after, Some(2), "https://wanted.test/", false).is_some());
}

#[test]
fn ambiguous_dispatched_create_requires_one_url_matching_delta() {
    let before = [tab(1, 10, true, "https://before.test")];
    let after = [
        before[0].clone(),
        tab(2, 10, false, "https://wanted.test/path#loaded"),
        tab(3, 20, true, "https://other.test"),
    ];
    let (selected, recovered) =
        select_created_tab(&before, &after, None, "https://wanted.test/path", true).unwrap();
    assert_eq!(selected.id, 2);
    assert!(recovered);
    assert!(select_created_tab(&before, &after, None, "https://wanted.test/path", false).is_none());
    let duplicate_match = [
        before[0].clone(),
        tab(2, 10, false, "https://wanted.test/path#one"),
        tab(3, 20, true, "https://wanted.test/path#two"),
    ];
    assert!(
        select_created_tab(
            &before,
            &duplicate_match,
            None,
            "https://wanted.test/path",
            true
        )
        .is_none()
    );
}

#[test]
fn inconsistent_returned_id_can_only_recover_by_unique_url_delta() {
    let before = [tab(1, 10, true, "https://before.test")];
    let after = [
        before[0].clone(),
        tab(2, 10, false, "https://wanted.test/path"),
        tab(3, 10, false, "https://other.test"),
    ];
    let (selected, recovered) =
        select_created_tab(&before, &after, Some(99), "https://wanted.test/path", true).unwrap();
    assert_eq!(selected.id, 2);
    assert!(recovered);
    let (_, recovered) =
        select_created_tab(&before, &after, Some(2), "https://wanted.test/path", true).unwrap();
    assert!(!recovered);
}

#[test]
fn restore_target_is_bound_to_created_window() {
    let before = [
        tab(1, 10, true, "https://wrong.test"),
        tab(2, 20, true, "https://right.test"),
    ];
    assert_eq!(active_in_window(&before, 20).unwrap().id, 2);
}

#[test]
fn focus_takeover_is_not_closed_or_restored() {
    let owned = TemporaryBrowserTab {
        id: 3,
        foreground: false,
        recovered_create: false,
        epoch: 1,
        window_id: Some(10),
        requested_url: "https://owned.test/".into(),
        navigation_allowed: false,
        restore_allowed: false,
        restore: None,
    };
    assert_eq!(
        close_policy(&owned, &tab(3, 10, true, "https://owned.test"), false),
        ClosePolicy::UserTakeover
    );
    assert_eq!(
        close_policy(&owned, &tab(3, 10, false, "https://owned.test"), false),
        ClosePolicy::Close
    );
    let mut foreground_owned = owned;
    foreground_owned.foreground = true;
    foreground_owned.restore_allowed = true;
    foreground_owned.restore = Some(RestoreTarget {
        id: 1,
        window_id: 10,
    });
    assert_eq!(
        close_policy(
            &foreground_owned,
            &tab(3, 10, false, "https://owned.test"),
            false
        ),
        ClosePolicy::Close
    );
    assert_eq!(
        close_policy(
            &foreground_owned,
            &tab(3, 10, true, "https://owned.test"),
            true
        ),
        ClosePolicy::UserTakeover
    );
}

#[test]
fn identity_rejects_reconnect_window_move_and_id_reuse_url() {
    let owned = TemporaryBrowserTab {
        id: 3,
        foreground: true,
        recovered_create: false,
        epoch: 7,
        window_id: Some(10),
        requested_url: "https://owned.test/path".into(),
        navigation_allowed: false,
        restore_allowed: true,
        restore: Some(RestoreTarget {
            id: 1,
            window_id: 10,
        }),
    };
    assert!(identity_conflict(&owned, &tab(3, 10, true, "https://owned.test/path"), 7).is_none());
    assert_eq!(
        identity_conflict(&owned, &tab(3, 10, true, "https://owned.test/path"), 8),
        Some("connection_epoch_changed")
    );
    assert_eq!(
        identity_conflict(&owned, &tab(3, 11, true, "https://owned.test/path"), 7),
        Some("browser_window_changed")
    );
    assert_eq!(
        identity_conflict(&owned, &tab(3, 10, true, "https://reused.test"), 7),
        Some("document_identity_changed")
    );

    let mut navigable = owned;
    navigable.navigation_allowed = true;
    assert!(identity_conflict(&navigable, &tab(3, 10, false, "https://later.test"), 7).is_none());
}

#[test]
fn moved_restore_target_is_rejected_without_changing_close_policy() {
    let owned = TemporaryBrowserTab {
        id: 3,
        foreground: true,
        recovered_create: false,
        epoch: 7,
        window_id: Some(10),
        requested_url: "https://owned.test/".into(),
        navigation_allowed: false,
        restore_allowed: true,
        restore: Some(RestoreTarget {
            id: 1,
            window_id: 10,
        }),
    };
    let inventory = [
        tab(1, 11, true, "https://restore.test"),
        tab(3, 10, true, "https://owned.test"),
    ];
    assert!(validate_restore_target(&inventory, &owned).is_err());
    assert_eq!(
        close_policy(&owned, &inventory[1], false),
        ClosePolicy::CloseAndRestore
    );
}

#[test]
fn cleanup_capability_requires_complete_exact_tab_cdp_fallback() {
    assert!(cleanup_capability_available(true, false, false));
    assert!(cleanup_capability_available(false, true, true));
    assert!(!cleanup_capability_available(false, true, false));
    assert!(!cleanup_capability_available(false, false, true));
}

#[test]
fn inventory_and_canonical_identity_fail_closed() {
    assert!(parse_tab_inventory(&json!({})).is_err());
    assert!(parse_tab_inventory(&json!([{"id": 1}])).is_err());
    assert_eq!(
        canonical_url("https://host.test/a#one"),
        canonical_url("https://host.test/a#two")
    );
}

#[test]
fn legacy_inventory_without_window_or_pending_url_is_safe_for_background_leases() {
    let parsed = parse_tab_inventory(&json!([{
        "id": 7,
        "active": false,
        "url": "https://source.test/"
    }]))
    .unwrap();
    assert_eq!(parsed[0].window_id, None);
    assert_eq!(parsed[0].pending_url, None);
    assert!(
        parse_tab_inventory(&json!([{
            "id": 7,
            "active": false,
            "url": "https://source.test/",
            "windowId": 0
        }]))
        .is_err()
    );
}

#[test]
fn open_ambiguity_is_typed_not_inferred_from_message_text() {
    assert!(open_effect_ambiguous(&open_error(true, "opaque")));
    assert!(!open_effect_ambiguous(&anyhow::anyhow!("ambiguous")));
}

#[test]
fn tab_leases_follow_the_requested_visibility_preference_with_fallback() {
    assert_eq!(preferred_tab_foreground(true, true, true), Some(false));
    assert_eq!(preferred_tab_foreground(false, true, true), Some(true));
    assert_eq!(preferred_tab_foreground(true, true, false), Some(true));
    assert_eq!(preferred_tab_foreground(true, false, false), Some(false));
    assert_eq!(preferred_tab_foreground(false, false, true), None);
}

#[test]
fn foreground_cleanup_requires_window_identity_before_create() {
    let foreground_cleanup = LeasePolicy {
        foreground: true,
        navigation_allowed: false,
        cleanup_required: true,
    };
    let background_cleanup = LeasePolicy {
        foreground: false,
        ..foreground_cleanup
    };
    let persistent_foreground = LeasePolicy {
        cleanup_required: false,
        ..foreground_cleanup
    };
    let legacy = [TabSnapshot {
        id: 1,
        window_id: None,
        active: true,
        url: Some("https://before.test/".into()),
        pending_url: None,
    }];

    assert!(require_foreground_precreate_identity(foreground_cleanup, true, &legacy).is_err());
    assert!(require_foreground_precreate_identity(foreground_cleanup, false, &[]).is_err());
    assert!(
        require_foreground_precreate_identity(
            foreground_cleanup,
            true,
            &[tab(1, 10, true, "https://before.test/")]
        )
        .is_ok()
    );
    assert!(require_foreground_precreate_identity(background_cleanup, false, &legacy).is_ok());
    assert!(require_foreground_precreate_identity(persistent_foreground, false, &legacy).is_ok());
}

#[test]
fn foreground_created_tab_without_window_identity_is_ambiguous() {
    let policy = LeasePolicy {
        foreground: true,
        navigation_allowed: false,
        cleanup_required: true,
    };
    let created = TabSnapshot {
        id: 2,
        window_id: None,
        active: true,
        url: Some("https://created.test/".into()),
        pending_url: None,
    };
    let error = require_foreground_created_identity(policy, &created).unwrap_err();
    assert!(open_effect_ambiguous(&error));
    assert!(
        require_foreground_created_identity(policy, &tab(2, 10, true, "https://created.test/"))
            .is_ok()
    );
}

#[test]
fn background_research_redirect_keeps_the_exact_owned_tab_closeable() {
    let owned = TemporaryBrowserTab {
        id: 3,
        foreground: false,
        recovered_create: false,
        epoch: 7,
        window_id: None,
        requested_url: "https://requested.test/".into(),
        navigation_allowed: true,
        restore_allowed: false,
        restore: None,
    };
    let redirected = TabSnapshot {
        id: 3,
        window_id: None,
        active: false,
        url: Some("https://redirected.test/".into()),
        pending_url: None,
    };
    assert!(identity_conflict(&owned, &redirected, 7).is_none());
    assert_eq!(close_policy(&owned, &redirected, false), ClosePolicy::Close);
}
