use super::*;
use crate::api::gemini_live::{response::collect_response, test_support::*};
use std::net::{TcpListener, TcpStream};
use std::sync::{atomic::AtomicBool, mpsc};
use tungstenite::{Message, WebSocket};

fn ready_server(
    control: &RequestControl,
    run: impl FnOnce(&mut WebSocket<native_tls::TlsStream<TcpStream>>) + Send + 'static,
) -> (ReadyLiveSession, std::thread::JoinHandle<()>) {
    // Public test identity, trusted only by this loopback client.
    let cert = include_bytes!("../../client/fixtures/localhost-cert.pem");
    let key = include_bytes!("../../client/fixtures/localhost-test-key.pem");
    let acceptor =
        native_tls::TlsAcceptor::new(native_tls::Identity::from_pkcs8(cert, key).unwrap()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tungstenite::accept(acceptor.accept(tcp).unwrap()).unwrap();
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        socket
            .send(Message::Text(r#"{"setupComplete":{}}"#.into()))
            .unwrap();
        run(&mut socket);
    });
    let tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    let connector = native_tls::TlsConnector::builder()
        .add_root_certificate(native_tls::Certificate::from_pem(cert).unwrap())
        .build()
        .unwrap();
    let tls = connector.connect("localhost", tcp).unwrap();
    let url = format!("wss://localhost:{}/", address.port());
    let (socket, _) = tungstenite::client::client(url.as_str(), tls).unwrap();
    let connected = ConnectedLiveSocket::from_socket(socket);
    control
        .bind_transport(&connected.shutdown_handle().unwrap())
        .unwrap();
    let ready = connected
        .activate_with(
            serde_json::json!({"setup":{}}),
            OpenOptions {
                setup_timeout: Duration::from_secs(3),
                setup_read_timeout: MAX_CANCELLATION_POLL,
                active_read_timeout: MAX_CANCELLATION_POLL,
            },
            || control.is_retired(),
        )
        .unwrap();
    (ready, server)
}

#[test]
fn accepted_tls_socket_is_closed_when_response_is_abandoned_before_output() {
    let manager = GeminiLiveManager::new();
    let (response, _) = request(&manager, None);
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (mut ready, server) = ready_server(&queued.control, move |socket| {
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        entered_tx.send(()).unwrap();
        assert!(
            socket.read().is_err(),
            "abandoned accepted transport remained open"
        );
    });
    let worker = std::thread::spawn(move || {
        let outcome = serve(&mut ready, &queued, ServeAttempt::new(false));
        let _ = ready.close();
        outcome
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    drop(response);
    assert_eq!(worker.join().unwrap(), ServeOutcome::Retired);
    server.join().unwrap();
}

#[test]
fn cancellation_after_tls_output_rejects_late_chunks_and_closes_owner() {
    let manager = GeminiLiveManager::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let (response, options) = request(&manager, Some(cancel.clone()));
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let (mut ready, server) = ready_server(&queued.control, move |socket| {
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        socket
            .send(Message::Text(
                r#"{"serverContent":{"modelTurn":{"parts":[{"text":"first"}]}}}"#.into(),
            ))
            .unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let _ = socket.send(Message::Text(
            r#"{"serverContent":{"modelTurn":{"parts":[{"text":"late"}]},"turnComplete":true}}"#
                .into(),
        ));
        assert!(
            socket.read().is_err(),
            "cancelled accepted transport remained open"
        );
    });
    let worker = std::thread::spawn(move || {
        let outcome = serve(&mut ready, &queued, ServeAttempt::new(false));
        let _ = ready.close();
        outcome
    });
    let mut delivered = Vec::new();
    let result = collect_response(response, &options, true, "en", |text| {
        delivered.push(text.to_string());
        cancel.store(true, Ordering::SeqCst);
        release_tx.send(()).unwrap();
    });
    assert!(result.is_err());
    assert_eq!(delivered, ["first"]);
    assert_eq!(worker.join().unwrap(), ServeOutcome::Retired);
    server.join().unwrap();
}

fn fast_policy() -> ResponsePolicy {
    ResponsePolicy {
        first_output_ms: 300,
        content_idle_ms: 100,
        hard_response_ms: 2_000,
    }
}

fn silent_server_outcome(reused_warm: bool) -> (ServeOutcome, Option<LiveEvent>) {
    let manager = GeminiLiveManager::new();
    let (response, _) = request(&manager, None);
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let (mut ready, server) = ready_server(&queued.control, move |socket| {
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        let _ = done_rx.recv_timeout(Duration::from_secs(3));
    });
    let attempt = ServeAttempt {
        policy: fast_policy(),
        reused_warm,
    };
    let outcome = serve(&mut ready, &queued, attempt);
    let _ = ready.close();
    let _ = done_tx.send(());
    server.join().unwrap();
    (outcome, response.receiver.try_recv().ok())
}

#[test]
fn silent_warm_session_is_handed_back_for_one_cold_retry() {
    let (outcome, event) = silent_server_outcome(true);
    assert!(
        matches!(&outcome, ServeOutcome::Stale(detail) if detail.contains("no server frame")),
        "{outcome:?}"
    );
    assert!(
        event.is_none(),
        "stale warm attempt must not fail the caller"
    );
}

#[test]
fn silent_cold_session_reports_response_timeout() {
    let (outcome, event) = silent_server_outcome(false);
    assert_eq!(outcome, ServeOutcome::Failed);
    assert!(matches!(event, Some(LiveEvent::Error(error)) if error == "Response timeout"));
}

#[test]
fn final_stale_close_reports_the_close_detail_to_the_caller() {
    let manager = GeminiLiveManager::new();
    let (response, _) = request(&manager, None);
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    let (mut ready, server) = ready_server(&queued.control, |socket| {
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        socket
            .close(Some(tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::Policy,
                reason: "policy violation".into(),
            }))
            .unwrap();
        while socket.read().is_ok() {}
    });
    let outcome = serve(&mut ready, &queued, ServeAttempt::new(false));
    assert!(
        matches!(&outcome, ServeOutcome::Stale(detail) if detail.contains("1008")),
        "{outcome:?}"
    );
    report_unrecovered_stale(&queued, &outcome);
    let _ = ready.close();
    server.join().unwrap();
    match response.receiver.try_recv() {
        Ok(LiveEvent::Error(error)) => {
            assert!(error.contains("code 1008: policy violation"), "{error}");
        }
        other => panic!("expected a final close error, got {other:?}"),
    }
}
