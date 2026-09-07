use super::*;

#[test]
fn readiness_stays_preparing_across_retry_gaps() {
    let sequence = [
        ("preparing", true),
        ("unavailable", true),
        ("preparing", true),
        ("unavailable", true),
        ("ready", true),
        ("ready", false),
    ];
    let displayed: Vec<_> = sequence
        .into_iter()
        .map(|(observed, active)| displayed_readiness(observed, active))
        .collect();
    assert_eq!(
        displayed,
        [
            "preparing",
            "preparing",
            "preparing",
            "preparing",
            "ready",
            "ready"
        ]
    );
}

#[test]
fn stopped_preparation_does_not_hide_unavailability() {
    assert_eq!(displayed_readiness("unavailable", false), "unavailable");
    assert_eq!(displayed_readiness("ready", true), "ready");
}

#[test]
fn readiness_parser_accepts_only_the_public_state_contract() {
    assert!(supported_readiness_tool("3d"));
    assert!(supported_readiness_tool("svg"));
    assert!(!supported_readiness_tool("image"));
    assert!(!supported_readiness_tool("unknown"));
    assert_eq!(
        parse_readiness(br#"{"ok":true,"result":{"state":"ready"}}"#).as_deref(),
        Some("ready")
    );
    assert_eq!(
        parse_readiness(
            br#"{"event":"progress"}
{"ok":true,"result":{"state":"preparing"}}"#,
        )
        .as_deref(),
        Some("preparing")
    );
    assert!(parse_readiness(br#"{"ok":true,"result":{"state":"ready","extra":1}}"#).is_none());
    assert!(parse_readiness(br#"{"ok":true,"result":{"state":"unknown"}}"#).is_none());
}

#[test]
fn every_demand_maintains_the_global_three_slot_reserve() {
    assert_eq!(desired_readiness_capacity(0), 3);
    assert_eq!(desired_readiness_capacity(1), 3);
    assert_eq!(desired_readiness_capacity(2), 3);
    assert_eq!(desired_readiness_capacity(100), 3);
}

#[test]
fn an_old_readiness_worker_cannot_remove_its_replacement() {
    let task = |stopped| {
        Arc::new(ReadinessTask {
            stop: Arc::new(AtomicBool::new(stopped)),
            desired: AtomicUsize::new(4),
            install_if_missing: AtomicBool::new(false),
        })
    };
    let previous = task(true);
    let replacement = task(false);
    let mut in_flight =
        std::collections::HashMap::from([("image".to_string(), replacement.clone())]);

    remove_readiness_if_current(&mut in_flight, "image", &previous);
    assert!(Arc::ptr_eq(in_flight.get("image").unwrap(), &replacement));

    remove_readiness_if_current(&mut in_flight, "image", &replacement);
    assert!(!in_flight.contains_key("image"));
}
