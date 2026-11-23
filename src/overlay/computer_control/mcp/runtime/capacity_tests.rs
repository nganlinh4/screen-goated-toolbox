use super::super::client::native_tests::Fixture;
use super::*;

fn tool(name: &str) -> McpTool {
    McpTool {
        name: name.into(),
        description: "Operation".into(),
        input_schema: json!({"type":"object","properties":{}}),
        annotations: Default::default(),
    }
}

fn connected(fixture: &Fixture) -> Arc<McpClient> {
    Arc::new(
        McpClient::spawn_managed(&fixture.launch("stay"), None)
            .unwrap()
            .0,
    )
}

#[test]
fn aggregate_count_rejects_before_normalization_and_recovers_without_dropping_tools() {
    let fixture = Fixture::new();
    let first = connected(&fixture);
    let second = connected(&fixture);
    let store = parking_lot::Mutex::new(Manager::default());
    let tools = (0..64)
        .map(|i| tool(&format!("operation_{i}")))
        .collect::<Vec<_>>();
    assert!(register_connection(
        &store,
        "first",
        &first,
        tools.clone(),
        None
    ));
    assert!(register_connection(
        &store,
        "second",
        &second,
        tools.clone(),
        None
    ));
    assert_eq!(
        declarations::build(&store, |snapshot| Ok(direct_declarations(snapshot)))
            .unwrap()
            .len(),
        128
    );
    let declared_name = store
        .lock()
        .routes
        .iter()
        .find(|(_, route)| route.integration_id == "first" && route.tool_name == "operation_0")
        .unwrap()
        .0
        .clone();
    let mut larger = tools.clone();
    larger.push(tool("additional_operation"));
    assert!(replace_catalog(
        &store,
        "second",
        second.connection_token(),
        larger
    ));
    let error = declarations::build(&store, |_| panic!("overflow reached the engine")).unwrap_err();
    assert!(error.downcast_ref::<capacity::CatalogCapacity>().is_some());
    assert!(error.to_string().contains("129 tools, limit 128"));
    assert!(store.lock().routes.is_empty());
    assert!(is_connected_in(&store, "first"));
    assert!(is_connected_in(&store, "second"));
    assert_eq!(
        catalog_error_in(&store, "first"),
        catalog_error_in(&store, "second")
    );
    assert!(catalog_error_in(&store, "first").is_some());
    assert!(catalog_error_in(&store, "unrelated").is_none());
    assert!(first.is_alive() && second.is_alive());
    let rejected = dispatch_in(
        &store,
        &declared_name,
        &json!({}),
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(rejected["ok"], false);
    assert!(
        super::super::client::native_tests::rpc(&first, "fixture/status", json!({}))["calls"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(replace_catalog(
        &store,
        "second",
        second.connection_token(),
        tools
    ));
    assert_eq!(
        declarations::build(&store, |snapshot| Ok(direct_declarations(snapshot)))
            .unwrap()
            .len(),
        128
    );
    assert!(catalog_error_in(&store, "first").is_none());
    assert_eq!(store.lock().routes.len(), 128);
    let response = dispatch_in(
        &store,
        &declared_name,
        &json!({"receipt":"recovered"}),
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["raw"]["name"], "operation_0");
    assert_eq!(response["raw"]["arguments"]["receipt"], "recovered");
    assert_eq!(
        super::super::client::native_tests::rpc(&first, "fixture/status", json!({}))["calls"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    first.shutdown();
    second.shutdown();
}

#[test]
fn combined_schema_bytes_reject_the_whole_catalog_and_survive_session_teardown() {
    let fixture = Fixture::new();
    let client = connected(&fixture);
    let store = parking_lot::Mutex::new(Manager::default());
    let mut large = tool("large_operation");
    // Each enum member is within the schema contract; escaping makes encoded
    // bytes substantially larger than character counts.
    large.input_schema = json!({"type":"object","properties":{
        "choice":{"type":"string","enum":(0..20).map(|i| format!("{i}{}", "\\\"".repeat(750))).collect::<Vec<_>>()}
    }});
    let mut other = large.clone();
    other.name = "other_operation".into();
    assert!(register_connection(
        &store,
        "provider",
        &client,
        vec![large, other],
        None
    ));
    let error = declarations::build(&store, |snapshot| {
        let (declarations, routes) = direct_declarations(snapshot);
        assert_eq!(declarations.len(), 2);
        for declaration in &declarations {
            capacity::check_declarations(std::slice::from_ref(declaration)).unwrap();
        }
        Ok((declarations, routes))
    })
    .unwrap_err();
    assert!(error.downcast_ref::<capacity::CatalogCapacity>().is_some());
    assert!(error.to_string().contains("encoded bytes"));
    assert!(store.lock().routes.is_empty());
    assert!(client.is_alive());
    assert!(disconnect_all_in(&store, |connection| connection
        .client
        .shutdown()));
    assert!(
        catalog_error_in(&store, "provider").is_some(),
        "failed setup teardown must retain the actionable diagnostic"
    );
    assert!(store.lock().connected.is_empty());
    assert!(detach_connection(&store, "provider", None).is_none());
    assert!(
        catalog_error_in(&store, "provider").is_none(),
        "explicit removal must clear a diagnostic even after session teardown"
    );
}

#[test]
fn integration_capacity_checks_include_empty_catalogs() {
    let mut snapshot = (0..32)
        .map(|i| (format!("provider_{i}"), i, vec![]))
        .collect::<ConnSnapshot>();
    capacity::check_snapshot(&snapshot).unwrap();
    snapshot.push(("additional_provider".into(), 33, vec![]));
    let error = capacity::check_snapshot(&snapshot).unwrap_err();
    assert!(error.to_string().contains("33 integrations, limit 32"));
}

#[test]
fn stale_oversized_declarations_do_not_poison_a_replaced_catalog() {
    let fixture = Fixture::new();
    let client = connected(&fixture);
    let store = parking_lot::Mutex::new(Manager::default());
    assert!(register_connection(
        &store,
        "provider",
        &client,
        vec![tool("old")],
        None
    ));
    let mut calls = 0;
    let declarations = declarations::build(&store, |snapshot| {
        calls += 1;
        if calls == 1 {
            assert!(replace_catalog(
                &store,
                "provider",
                client.connection_token(),
                vec![tool("new")]
            ));
            Ok((vec![json!({"payload":"x".repeat(100_000)})], HashMap::new()))
        } else {
            Ok(direct_declarations(snapshot))
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(declarations.len(), 1);
    assert!(
        store
            .lock()
            .routes
            .values()
            .all(|route| route.tool_name == "new")
    );
    assert!(catalog_error_in(&store, "provider").is_none());
    client.shutdown();
}
