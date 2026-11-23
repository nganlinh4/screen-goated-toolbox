use super::super::client::native_tests::Fixture;
use super::*;
use std::time::Duration;

const ID: &str = "future_provider";

fn tools(read_only: bool) -> Vec<McpTool> {
    vec![tool("operation", read_only)]
}
fn tool(name: &str, read_only: bool) -> McpTool {
    McpTool {
        name: name.into(),
        description: "Operation".into(),
        input_schema: json!({"type":"object","properties":{"value":{"type":"string"}}}),
        annotations: super::super::client::McpToolAnnotations {
            read_only: Some(read_only),
            ..Default::default()
        },
    }
}
fn client(fixture: &Fixture) -> Arc<McpClient> {
    Arc::new(
        McpClient::spawn_managed(&fixture.launch("stay"), None)
            .unwrap()
            .0,
    )
}
fn populated(fixture: &Fixture) -> (Arc<parking_lot::Mutex<Manager>>, Arc<McpClient>) {
    let client = client(fixture);
    let store = Arc::new(parking_lot::Mutex::new(Manager::default()));
    assert!(register_connection(&store, ID, &client, tools(true), None));
    declarations::build(&store, |snapshot| Ok(direct_declarations(snapshot))).unwrap();
    (store, client)
}
fn check_read_is_available(store: &Arc<parking_lot::Mutex<Manager>>, expected: bool) {
    let (tx, rx) = mpsc::channel();
    let store = store.clone();
    let reader = std::thread::spawn(move || tx.send(is_connected_in(&store, ID)).unwrap());
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
    reader.join().unwrap();
}

#[test]
fn normalization_does_not_hold_ui_lock_and_retries_the_full_changed_catalog() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || {
        let mut calls = 0;
        let result = declarations::build(&worker_store, |snapshot| {
            calls += 1;
            if calls == 1 {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5))?;
            }
            Ok(direct_declarations(snapshot))
        });
        (result, calls)
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    check_read_is_available(&store, true);
    let token = client.connection_token();
    assert!(invalidate_catalog(&store, ID, token).is_some());
    check_read_is_available(&store, false);
    assert!(replace_catalog(
        &store,
        ID,
        token,
        vec![tool("operation", false), tool("new_operation", true)]
    ));
    resume_tx.send(()).unwrap();
    let (result, calls) = worker.join().unwrap();
    assert_eq!(calls, 2);
    assert_eq!(result.unwrap().len(), 2);
    let manager = store.lock();
    assert_eq!(manager.routes.len(), 2);
    let route = manager
        .routes
        .values()
        .find(|route| route.tool_name == "operation")
        .unwrap();
    assert_eq!(route.annotations.read_only, Some(false));
    drop(manager);
    client.shutdown();
}

#[test]
fn disconnect_stops_outside_ui_lock_and_old_normalization_cannot_resurrect_routes() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker_store = store.clone();
    let builder = std::thread::spawn(move || {
        let mut calls = 0;
        declarations::build(&worker_store, |snapshot| {
            calls += 1;
            if calls == 1 {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5))?;
            }
            Ok(direct_declarations(snapshot))
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (stop_tx, stop_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let stop_store = store.clone();
    let retire = std::thread::spawn(move || {
        disconnect_all_in(&stop_store, |connection| {
            stop_tx.send(()).unwrap();
            finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            connection.client.shutdown();
        })
    });
    stop_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    check_read_is_available(&store, false);
    assert!(store.lock().routes.is_empty());
    resume_tx.send(()).unwrap();
    assert!(builder.join().unwrap().unwrap().is_empty());
    assert!(store.lock().routes.is_empty());
    finish_tx.send(()).unwrap();
    assert!(retire.join().unwrap());
    assert!(!client.is_alive());
}

#[test]
fn replacement_generation_rejects_old_refresh_removal_and_normalization() {
    let old_fixture = Fixture::new();
    let new_fixture = Fixture::new();
    let (store, old_client) = populated(&old_fixture);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || {
        let mut calls = 0;
        declarations::build(&worker_store, |snapshot| {
            calls += 1;
            if calls == 1 {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5))?;
            }
            Ok(direct_declarations(snapshot))
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let replacement = client(&new_fixture);
    assert!(register_connection(
        &store,
        ID,
        &replacement,
        tools(false),
        None
    ));
    assert!(!replace_catalog(
        &store,
        ID,
        old_client.connection_token(),
        tools(true)
    ));
    assert!(detach_connection(&store, ID, Some(old_client.connection_token())).is_none());
    resume_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap().len(), 1);
    assert!(
        store
            .lock()
            .routes
            .values()
            .all(
                |route| route.connection_token == replacement.connection_token()
                    && route.annotations.read_only == Some(false)
            )
    );
    old_client.shutdown();
    replacement.shutdown();
}

#[test]
fn native_liveness_change_invalidates_input_without_a_manager_revision_change() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let revision = store.lock().revision;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || {
        let mut calls = 0;
        let result = declarations::build(&worker_store, |snapshot| {
            calls += 1;
            if calls == 1 {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5))?;
            }
            Ok(direct_declarations(snapshot))
        });
        (result, calls)
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    client.shutdown();
    assert_eq!(store.lock().revision, revision);
    resume_tx.send(()).unwrap();
    let (result, calls) = worker.join().unwrap();
    assert_eq!(calls, 2);
    assert!(result.unwrap().is_empty());
    assert!(store.lock().routes.is_empty());
}

#[test]
fn normalization_failure_preserves_a_current_route_set_without_returning_a_fallback() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let previous = store.lock().routes.clone();
    let error = declarations::build(&store, |_| {
        assert!(store.try_lock().is_some());
        anyhow::bail!("normalization unavailable")
    })
    .unwrap_err();
    assert!(error.to_string().contains("normalization unavailable"));
    assert_eq!(store.lock().routes, previous);
    client.shutdown();
}

#[test]
fn catalog_churn_is_bounded_and_never_publishes_a_partial_candidate() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let mut calls = 0;
    let error = declarations::build(&store, |snapshot| {
        calls += 1;
        assert!(calls <= 8, "catalog retries are not bounded");
        assert!(replace_catalog(
            &store,
            ID,
            client.connection_token(),
            vec![tool(&format!("operation_{calls}"), false)]
        ));
        Ok(direct_declarations(snapshot))
    })
    .unwrap_err();
    assert!(
        error
            .downcast_ref::<declarations::CatalogChanged>()
            .is_some()
    );
    assert!(store.lock().routes.is_empty());
    assert!(calls > 1);
    assert_eq!(
        declarations::build(&store, |snapshot| Ok(direct_declarations(snapshot)))
            .unwrap()
            .len(),
        1
    );
    client.shutdown();
}

#[test]
fn concurrent_builders_wait_without_holding_the_ui_manager_lock() {
    let fixture = Fixture::new();
    let (store, client) = populated(&fixture);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let first_store = store.clone();
    let first = std::thread::spawn(move || {
        declarations::build(&first_store, |snapshot| {
            entered_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(5))?;
            Ok(direct_declarations(snapshot))
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (second_tx, second_rx) = mpsc::channel();
    let second_store = store.clone();
    let second = std::thread::spawn(move || {
        waiting_tx.send(()).unwrap();
        declarations::build(&second_store, |snapshot| {
            second_tx.send(()).unwrap();
            Ok(direct_declarations(snapshot))
        })
    });
    waiting_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(second_rx.recv_timeout(Duration::from_millis(100)).is_err());
    check_read_is_available(&store, true);
    resume_tx.send(()).unwrap();
    assert_eq!(first.join().unwrap().unwrap().len(), 1);
    second_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(second.join().unwrap().unwrap().len(), 1);
    client.shutdown();
}

#[test]
fn a_stopped_registration_owner_cannot_replace_a_current_client() {
    let fixture = Fixture::new();
    let rejected_fixture = Fixture::new();
    let (store, current) = populated(&fixture);
    let rejected = client(&rejected_fixture);
    let revision = store.lock().revision;
    let routes = store.lock().routes.clone();
    assert!(!register_connection(
        &store,
        ID,
        &rejected,
        tools(false),
        Some(&AtomicBool::new(true))
    ));
    assert_eq!(store.lock().revision, revision);
    assert_eq!(store.lock().routes, routes);
    assert_eq!(
        store.lock().connected[ID].client.connection_token(),
        current.connection_token()
    );
    check_read_is_available(&store, true);
    current.shutdown();
    rejected.shutdown();
}
