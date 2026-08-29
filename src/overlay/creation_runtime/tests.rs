use super::*;

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
fn accepted_demand_does_not_add_a_speculative_warm_reserve() {
    assert_eq!(desired_readiness_capacity(0), 1);
    assert_eq!(desired_readiness_capacity(1), 1);
    assert_eq!(desired_readiness_capacity(2), 2);
    assert_eq!(desired_readiness_capacity(100), 2);
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
