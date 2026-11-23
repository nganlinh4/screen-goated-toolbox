use super::super::client::native_tests::{self as native, Fixture};
use super::*;
use crate::overlay::computer_control::effect_receipt::EffectStatus;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

struct Harness {
    client: Arc<McpClient>,
    store: Arc<parking_lot::Mutex<Manager>>,
    tool: String,
    _fixture: Fixture,
}

impl Harness {
    fn new(mode: &'static str) -> Self {
        let fixture = Fixture::new();
        let client = Arc::new(
            McpClient::spawn_managed(&fixture.launch(mode), None)
                .unwrap()
                .0,
        );
        let tools = client.list_tools().unwrap();
        let store = Arc::new(parking_lot::Mutex::new(Manager::default()));
        assert!(register_connection(
            &store, "provider", &client, tools, None
        ));
        declarations::build(&store, |snapshot| Ok(direct_declarations(snapshot))).unwrap();
        let tool = store.lock().routes.keys().next().unwrap().clone();
        Self {
            client,
            store,
            tool,
            _fixture: fixture,
        }
    }

    fn start(
        &self,
        args: Value,
        cancel: Arc<AtomicBool>,
    ) -> (mpsc::Receiver<Value>, JoinHandle<()>) {
        let store = self.store.clone();
        let tool = self.tool.clone();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = dispatch_in(&store, &tool, &args, &cancel).unwrap();
            let _ = tx.send(result);
        });
        (rx, worker)
    }

    fn collect<T>(&self, rx: mpsc::Receiver<T>, worker: JoinHandle<()>) -> T {
        let first = rx.recv_timeout(Duration::from_secs(2));
        if first.is_err() {
            self.client.shutdown();
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
        wait(|| worker.is_finished());
        worker.join().unwrap();
        first.expect("action cancellation did not release MCP dispatch within its budget")
    }

    fn blocked_write(
        &self,
        cancel: Arc<AtomicBool>,
    ) -> (mpsc::Receiver<anyhow::Result<Value>>, JoinHandle<()>) {
        native::rpc(&self.client, "fixture/block_input", json!({}));
        let client = self.client.clone();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = native::rpc_owned(
                &client,
                "fixture/echo",
                json!({"value":"x".repeat(512 * 1024)}),
                &cancel,
            );
            let _ = tx.send(result);
        });
        wait(|| native::writing(&self.client));
        (rx, worker)
    }

    fn retire(&self) {
        self.client.shutdown();
        native::wait_for_tree(&self.client);
        wait(|| native::writer_finished(&self.client));
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.client.shutdown();
    }
}

fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "native cancellation fixture did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_cancelled_action_cannot_issue_an_rpc_and_proves_no_effect() {
    let harness = Harness::new("stay");
    let cancel = Arc::new(AtomicBool::new(true));
    let result = dispatch_in(&harness.store, &harness.tool, &json!({}), &cancel).unwrap();
    assert_eq!(result["cancelled"], true);
    assert_eq!(
        EffectStatus::after_dispatch(&result, true),
        EffectStatus::ProvenNoEffect
    );
    let status = native::rpc(&harness.client, "fixture/status", json!({}));
    assert!(status["calls"].as_array().unwrap().is_empty());
    assert!(status["cancelled"].as_array().unwrap().is_empty());
    assert_eq!(native::pending_count(&harness.client), 0);
    harness.client.shutdown();
    native::wait_for_tree(&harness.client);
}

#[test]
fn an_inflight_call_cancels_without_reusing_its_late_response_or_killing_a_healthy_peer() {
    let harness = Harness::new("deferred-call");
    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, worker) = harness.start(json!({}), cancel.clone());
    let mut issued = Value::Null;
    wait(|| {
        let status = native::rpc(&harness.client, "fixture/status", json!({}));
        if let Some(id) = status["calls"].as_array().unwrap().first() {
            issued = id.clone();
            true
        } else {
            false
        }
    });
    cancel.store(true, Ordering::SeqCst);
    let result = harness.collect(rx, worker);
    assert_eq!(result["cancelled"], true);
    assert_eq!(
        EffectStatus::after_dispatch(&result, true),
        EffectStatus::MayHaveOccurred
    );
    assert!(harness.client.is_alive());
    assert_eq!(native::pending_count(&harness.client), 0);
    let status = native::rpc(&harness.client, "fixture/status", json!({}));
    assert_eq!(status["cancelled"], json!([issued]));
    assert_eq!(
        native::rpc(&harness.client, "fixture/finish", json!({"current":true})),
        json!({"current":true})
    );
    assert_eq!(
        native::rpc(&harness.client, "fixture/echo", json!({"next":2})),
        json!({"next":2})
    );
    harness.client.shutdown();
    native::wait_for_tree(&harness.client);
}

#[test]
fn a_reply_deadline_retires_only_its_request_and_ignores_the_late_result() {
    let harness = Harness::new("deferred-call");
    let client = harness.client.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let error = client
            .call_tool_timeout("echo", &json!({}), Duration::from_millis(500))
            .unwrap_err();
        let _ = tx.send(super::super::client::format_error(&error));
    });
    let result = harness.collect(rx, worker);
    assert_eq!(result["code"], "ERR_MCP_REQUEST_TIMEOUT");
    assert_eq!(result["cancelled"], false);
    assert_eq!(
        EffectStatus::from_value(&result),
        EffectStatus::MayHaveOccurred
    );
    assert!(harness.client.is_alive());
    assert_eq!(native::pending_count(&harness.client), 0);
    let status = native::rpc(&harness.client, "fixture/status", json!({}));
    assert_eq!(status["calls"].as_array().unwrap().len(), 1);
    assert_eq!(status["cancelled"], status["calls"]);
    assert_eq!(
        native::rpc(&harness.client, "fixture/finish", json!({"current":true})),
        json!({"current":true})
    );
    assert_eq!(
        native::rpc(&harness.client, "fixture/echo", json!({"next":2})),
        json!({"next":2})
    );
    harness.retire();
}

#[test]
fn cancellation_during_a_write_to_a_blocked_peer_retires_the_transport() {
    let harness = Harness::new("stay");
    native::rpc(&harness.client, "fixture/block_input", json!({}));
    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, worker) = harness.start(json!({"value":"x".repeat(512 * 1024)}), cancel.clone());
    wait(|| native::writing(&harness.client));
    cancel.store(true, Ordering::SeqCst);
    let result = harness.collect(rx, worker);
    assert_eq!(result["cancelled"], true);
    assert_eq!(
        EffectStatus::after_dispatch(&result, true),
        EffectStatus::MayHaveOccurred
    );
    native::wait_for_tree(&harness.client);
    assert!(!harness.client.is_alive());
    assert_eq!(native::pending_count(&harness.client), 0);
}

#[test]
fn a_backpressured_action_cancels_without_interrupting_another_write() {
    let harness = Harness::new("stay");
    let first_cancel = Arc::new(AtomicBool::new(false));
    let (first_rx, first_worker) = harness.blocked_write(first_cancel.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, worker) = harness.start(json!({}), cancel.clone());
    wait(|| native::pending_count(&harness.client) == 2);
    cancel.store(true, Ordering::SeqCst);
    let result = harness.collect(rx, worker);
    assert_eq!(result["cancelled"], true);
    assert_eq!(
        EffectStatus::from_value(&result),
        EffectStatus::ProvenNoEffect
    );
    assert!(harness.client.is_alive());
    assert!(native::writing(&harness.client));
    assert_eq!(native::pending_count(&harness.client), 1);
    first_cancel.store(true, Ordering::SeqCst);
    assert!(harness.collect(first_rx, first_worker).is_err());
    harness.retire();
}

#[test]
fn a_backpressured_deadline_proves_no_effect_without_interrupting_another_write() {
    let harness = Harness::new("stay");
    let first_cancel = Arc::new(AtomicBool::new(false));
    let (first_rx, first_worker) = harness.blocked_write(first_cancel.clone());
    let client = harness.client.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let error = client
            .call_tool_timeout("echo", &json!({}), Duration::from_millis(100))
            .unwrap_err();
        let _ = tx.send(super::super::client::format_error(&error));
    });
    let result = harness.collect(rx, worker);
    assert_eq!(result["code"], "ERR_MCP_REQUEST_TIMEOUT");
    assert_eq!(
        EffectStatus::from_value(&result),
        EffectStatus::ProvenNoEffect
    );
    assert!(harness.client.is_alive());
    assert!(native::writing(&harness.client));
    assert_eq!(native::pending_count(&harness.client), 1);
    first_cancel.store(true, Ordering::SeqCst);
    assert!(harness.collect(first_rx, first_worker).is_err());
    harness.retire();
}

#[test]
fn a_write_deadline_also_retires_a_peer_that_never_reads() {
    let harness = Harness::new("stay");
    native::rpc(&harness.client, "fixture/block_input", json!({}));
    let client = harness.client.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let error = client
            .call_tool_timeout(
                "echo",
                &json!({"value":"x".repeat(512 * 1024)}),
                Duration::from_secs(1),
            )
            .unwrap_err();
        let _ = tx.send(super::super::client::format_error(&error));
    });
    wait(|| native::writing(&harness.client));
    let result = harness.collect(rx, worker);
    assert_eq!(result["code"], "ERR_MCP_REQUEST_TIMEOUT");
    assert_eq!(
        EffectStatus::from_value(&result),
        EffectStatus::MayHaveOccurred
    );
    assert!(!harness.client.is_alive());
    harness.retire();
}
