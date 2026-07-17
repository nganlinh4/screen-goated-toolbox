use super::*;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[test]
fn every_future_integration_tool_is_declared_and_routed_directly() {
    let snapshot = vec![(
        "future_provider".to_string(),
        17,
        vec![
            ToolSnapshot {
                name: "first_capability".to_string(),
                description: "First operation".to_string(),
                input_schema: json!({"type": "object", "properties": {"value": {"type": "string"}}}),
                annotations: super::super::client::McpToolAnnotations {
                    read_only: Some(true),
                    ..Default::default()
                },
            },
            ToolSnapshot {
                name: "second_capability".to_string(),
                description: "Second operation".to_string(),
                input_schema: json!({"type": "object", "required": ["count"], "properties": {"count": {"type": "integer"}}}),
                annotations: super::super::client::McpToolAnnotations {
                    read_only: Some(false),
                    ..Default::default()
                },
            },
        ],
    )];

    let (declarations, routes) = direct_declarations(&snapshot);
    assert_eq!(declarations.len(), 2);
    assert_eq!(routes.len(), 2);
    let first_name = declarations[0]["name"].as_str().unwrap();
    let second_name = declarations[1]["name"].as_str().unwrap();
    assert!(first_name.starts_with("mcp__future_provider__first_capability"));
    assert!(second_name.starts_with("mcp__future_provider__second_capability"));
    assert_eq!(declarations[1]["parameters"]["required"], json!(["count"]));
    assert_eq!(
        routes[second_name],
        ToolRoute {
            integration_id: "future_provider".to_string(),
            tool_name: "second_capability".to_string(),
            annotations: super::super::client::McpToolAnnotations {
                read_only: Some(false),
                ..Default::default()
            },
            connection_token: 17,
        }
    );
    assert_eq!(routes[first_name].annotations.read_only, Some(true));
    assert!(annotations_are_read_only(routes[first_name].annotations));
    assert!(!annotations_are_read_only(
        super::super::client::McpToolAnnotations {
            read_only: Some(true),
            destructive: Some(true),
            open_world: Some(false),
        }
    ));
}

#[test]
fn incompatible_schema_withholds_its_route_without_hiding_a_valid_neighbor() {
    let mut incompatible = json!({"type": "string"});
    for _ in 0..32 {
        incompatible = json!({"type": "array", "items": incompatible});
    }
    let snapshot = vec![(
        "future_provider".to_string(),
        19,
        vec![
            ToolSnapshot {
                name: "usable".to_string(),
                description: "Usable".to_string(),
                input_schema: json!({"type": "object"}),
                annotations: Default::default(),
            },
            ToolSnapshot {
                name: "incompatible".to_string(),
                description: "Too deep".to_string(),
                input_schema: incompatible,
                annotations: Default::default(),
            },
        ],
    )];

    let (declarations, routes) = direct_declarations(&snapshot);

    assert_eq!(declarations.len(), 1);
    assert_eq!(routes.len(), 1);
    assert!(declarations[0]["name"].as_str().unwrap().contains("usable"));
    assert!(routes.values().all(|route| route.tool_name == "usable"));
}

#[test]
fn try_dispatch_accepts_only_an_alive_current_catalog_route() {
    let route = ToolRoute {
        integration_id: "future_provider".to_string(),
        tool_name: "capability".to_string(),
        annotations: Default::default(),
        connection_token: 23,
    };

    assert!(route_is_current(
        &route,
        ConnectionStatus {
            connection_token: 23,
            catalog_valid: true,
            alive: true,
        }
    ));
}

#[test]
fn try_dispatch_rejects_stale_invalid_or_dead_routes() {
    let route = ToolRoute {
        integration_id: "future_provider".to_string(),
        tool_name: "capability".to_string(),
        annotations: super::super::client::McpToolAnnotations {
            read_only: Some(true),
            ..Default::default()
        },
        connection_token: 23,
    };

    for connection in [
        ConnectionStatus {
            connection_token: 24,
            catalog_valid: true,
            alive: true,
        },
        ConnectionStatus {
            connection_token: 23,
            catalog_valid: false,
            alive: true,
        },
        ConnectionStatus {
            connection_token: 23,
            catalog_valid: true,
            alive: false,
        },
    ] {
        assert!(!route_is_current(&route, connection));
        assert_eq!(route_read_only(&route, connection), None);
    }
}

#[test]
fn annotation_flip_is_unknown_during_invalidation_then_replaced_exactly() {
    let snapshot = |read_only| {
        vec![(
            "future_provider".to_string(),
            29,
            vec![ToolSnapshot {
                name: "same_capability".to_string(),
                description: "Same operation".to_string(),
                input_schema: json!({"type": "object"}),
                annotations: super::super::client::McpToolAnnotations {
                    read_only: Some(read_only),
                    ..Default::default()
                },
            }],
        )]
    };
    let (old_declarations, old_routes) = direct_declarations(&snapshot(true));
    let declared_name = old_declarations[0]["name"].as_str().unwrap();
    let current = ConnectionStatus {
        connection_token: 29,
        catalog_valid: true,
        alive: true,
    };
    assert_eq!(
        route_read_only(&old_routes[declared_name], current),
        Some(true)
    );

    assert_eq!(
        route_read_only(
            &old_routes[declared_name],
            ConnectionStatus {
                catalog_valid: false,
                ..current
            }
        ),
        None
    );

    let (new_declarations, new_routes) = direct_declarations(&snapshot(false));
    assert_eq!(new_declarations.len(), 1);
    assert_eq!(new_declarations[0]["name"], old_declarations[0]["name"]);
    assert_eq!(
        route_read_only(&new_routes[declared_name], current),
        Some(false)
    );
}

#[test]
fn refresh_failure_removes_current_connection_but_ignores_a_stale_worker() {
    let current = ConnectionStatus {
        connection_token: 31,
        catalog_valid: false,
        alive: true,
    };

    assert_eq!(
        refresh_disposition(Some(current), 31, false),
        RefreshDisposition::Remove
    );
    assert_eq!(
        refresh_disposition(Some(current), 30, false),
        RefreshDisposition::Ignore
    );
    assert_eq!(
        refresh_disposition(Some(current), 31, true),
        RefreshDisposition::Replace
    );
}

#[test]
fn disconnect_removal_targets_only_the_dead_connection_generation() {
    let replacement = ConnectionStatus {
        connection_token: 44,
        catalog_valid: true,
        alive: true,
    };

    assert_eq!(
        refresh_disposition(Some(replacement), 43, false),
        RefreshDisposition::Ignore
    );
    assert_eq!(
        refresh_disposition(Some(replacement), 44, false),
        RefreshDisposition::Remove
    );
}

#[test]
fn catalog_generation_preserves_a_new_edge_after_a_burst_is_consumed() {
    let clock = CatalogChangeClock::new();
    clock.mark();
    clock.mark();
    assert!(clock.changed());

    clock.clear();
    assert!(!clock.changed());

    clock.mark();
    assert!(clock.changed());
}

#[test]
fn list_changed_burst_coalesces_latest_dirty_state_and_ignores_old_client_events() {
    let (old_signal, old_events) = super::super::client_protocol::lifecycle_channel(49);
    old_signal.raise(ClientLifecycleKind::Disconnected);
    drop(old_signal);
    let mut handled = Vec::new();
    consume_lifecycle_events(old_events, 50, |kind| handled.push(kind));
    assert!(handled.is_empty());

    let (signal, events) = super::super::client_protocol::lifecycle_channel(50);
    for _ in 0..100_000 {
        signal.raise(ClientLifecycleKind::ToolsChanged);
    }
    drop(signal);
    consume_lifecycle_events(events, 50, |kind| handled.push(kind));

    assert_eq!(handled, [ClientLifecycleKind::ToolsChanged]);
}

#[test]
fn non_mcp_names_remain_available_to_native_dispatch() {
    assert_eq!(try_dispatch("future_native_capability", &json!({})), None);
}

#[test]
fn stopped_owner_cannot_register_after_attempt_settles() {
    let stop = Arc::new(AtomicBool::new(false));
    let registered = Arc::new(AtomicBool::new(false));
    let (settle_tx, settle_rx) = mpsc::channel();
    let worker_stop = Arc::clone(&stop);
    let worker_registered = Arc::clone(&registered);
    let worker = std::thread::spawn(move || {
        settle_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        register_if_owner_active(Some(&worker_stop), || {
            worker_registered.store(true, Ordering::SeqCst);
        })
    });

    stop.store(true, Ordering::SeqCst);
    settle_tx.send(()).unwrap();
    assert!(!worker.join().unwrap());
    assert!(!registered.load(Ordering::SeqCst));
}
