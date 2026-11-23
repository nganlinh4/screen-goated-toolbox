use super::*;
use std::cell::Cell;
use std::net::TcpListener;

const BOOTSTRAP: &str = "fixture bootstrap";

fn hello(protocol: u64, nonce: &str, has_secret: bool, has_bootstrap: bool) -> Hello {
    Hello::parse(&json!({
        "type": "hello", "bridgeProtocol": protocol, "clientNonce": nonce,
        "hasSecret": has_secret, "hasBootstrap": has_bootstrap,
    }))
}

fn nonce() -> String {
    "11".repeat(32)
}

#[test]
fn hello_without_protocol_or_flags_is_legacy_and_unpaired() {
    let parsed = Hello::parse(&json!({"type": "hello"}));
    assert_eq!(parsed.protocol, 1);
    assert!(!parsed.has_secret && !parsed.has_bootstrap);
    assert_eq!(
        choose_key(&parsed, Some(1), BOOTSTRAP, "durable"),
        KeyChoice::UpdateRequired { legacy: true }
    );
}

#[test]
fn malformed_nonce_requires_update_without_a_legacy_hint() {
    for bad in ["", "zz", &"A".repeat(64), &"1".repeat(63)] {
        assert_eq!(
            choose_key(
                &hello(bridge_auth::PROTOCOL, bad, true, true),
                Some(1),
                BOOTSTRAP,
                "durable"
            ),
            KeyChoice::UpdateRequired { legacy: false },
            "{bad}"
        );
    }
}

#[test]
fn bootstrap_key_is_offered_only_inside_an_open_window() {
    let both = hello(bridge_auth::PROTOCOL, &nonce(), true, true);
    assert_eq!(
        choose_key(&both, Some(7), BOOTSTRAP, "durable"),
        KeyChoice::Challenge {
            kind: "bootstrap",
            key: bridge_auth::bootstrap_key(BOOTSTRAP),
            bootstrap_generation: Some(7),
        }
    );
    let durable = KeyChoice::Challenge {
        kind: "secret",
        key: "durable".to_string(),
        bootstrap_generation: None,
    };
    assert_eq!(choose_key(&both, None, BOOTSTRAP, "durable"), durable);
    assert_eq!(choose_key(&both, Some(7), "", "durable"), durable);
    let bootstrap_only = hello(bridge_auth::PROTOCOL, &nonce(), false, true);
    assert_eq!(
        choose_key(&bootstrap_only, None, BOOTSTRAP, "durable"),
        KeyChoice::NotPaired
    );
    let neither = hello(bridge_auth::PROTOCOL, &nonce(), false, false);
    assert_eq!(
        choose_key(&neither, Some(7), BOOTSTRAP, "durable"),
        KeyChoice::NotPaired
    );
}

#[test]
fn only_the_exact_client_proof_for_this_challenge_authenticates() {
    let (client, server) = (nonce(), "22".repeat(32));
    let proof = |key: &str, side: &str, kind: &str| json!({"type": "auth", "mac": bridge_auth::proof(key, side, kind, &client, &server)});
    assert!(client_proof_matches(
        "durable",
        "secret",
        &client,
        &server,
        &proof("durable", "client", "secret")
    ));
    for forged in [
        json!({"type": "auth"}),
        json!({"type": "auth", "mac": ""}),
        json!({"type": "auth", "mac": 7}),
        proof("durable", "server", "secret"),
        proof("durable", "client", "bootstrap"),
        proof("other", "client", "secret"),
    ] {
        assert!(
            !client_proof_matches("durable", "secret", &client, &server, &forged),
            "{forged}"
        );
    }
    assert!(!client_proof_matches(
        "",
        "secret",
        &client,
        &server,
        &proof("", "client", "secret")
    ));
    let other_server = "33".repeat(32);
    assert!(!client_proof_matches(
        "durable",
        "secret",
        &client,
        &other_server,
        &proof("durable", "client", "secret")
    ));
}

/// Drive the production `do_pairing` over loopback with a client that answers
/// the challenge with `answer` and report whether a session was claimed.
fn rejected_over_loopback(answer: impl FnOnce(&str, &str) -> Value, origin_ok: bool) -> bool {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut socket =
            tungstenite::accept(transport::DeadlineStream::new(stream, deadline)).unwrap();
        let durable = bridge_auth::durable_key(BOOTSTRAP);
        let claims = Cell::new(0);
        let consumed = Cell::new(0);
        let claim = || -> Option<server::SessionGuard> {
            claims.set(claims.get() + 1);
            None
        };
        let hooks = PairingHooks {
            secret: &durable,
            bootstrap: BOOTSTRAP,
            pairing_generation: &|| None,
            consume_pairing: &|_| {
                consumed.set(consumed.get() + 1);
                true
            },
            failed_pairing: &|_| panic!("no reservation was taken"),
            origin_allowed: &|_| origin_ok,
            legacy_hint: &|| panic!("not legacy"),
            claim_session: &claim,
            proof_phase: Duration::from_secs(3),
        };
        let headers = tungstenite::http::HeaderMap::new();
        let result = do_pairing(&mut socket, deadline, &headers, &hooks).unwrap();
        (result.is_none(), claims.get(), consumed.get())
    });
    let stream = TcpStream::connect(address).unwrap();
    let (mut socket, _) = tungstenite::client(format!("ws://{address}"), stream).unwrap();
    let client = nonce();
    socket
        .send(Message::Text(
            json!({"type":"hello", "bridgeProtocol": bridge_auth::PROTOCOL, "clientNonce": client, "hasSecret": true})
                .to_string()
                .into(),
        ))
        .unwrap();
    let challenge: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    let server_nonce = challenge["nonce"].as_str().unwrap().to_string();
    socket
        .send(Message::Text(
            answer(&client, &server_nonce).to_string().into(),
        ))
        .unwrap();
    let (rejected, claims, consumed) = server.join().unwrap();
    assert_eq!(consumed, 0, "a rejected pairing must not consume a window");
    rejected && claims == 0
}

#[test]
fn durable_client_cannot_skip_or_reflect_the_proof_over_loopback() {
    let durable = bridge_auth::durable_key(BOOTSTRAP);
    assert!(rejected_over_loopback(|_, _| json!({"type": "auth"}), true));
    assert!(rejected_over_loopback(
        |client, server| json!({"type": "auth", "mac": bridge_auth::proof(&durable, "server", "secret", client, server)}),
        true
    ));
}

#[test]
fn valid_proof_from_a_disallowed_origin_never_claims_a_session() {
    let durable = bridge_auth::durable_key(BOOTSTRAP);
    assert!(rejected_over_loopback(
        |client, server| json!({"type": "auth", "mac": bridge_auth::proof(&durable, "client", "secret", client, server)}),
        false
    ));
}

/// How long the production `do_pairing` holds a peer that upgrades and then
/// stops after sending `sent` messages.
fn hold_time_of_idle_peer(sent: &[Value]) -> Duration {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let overall = Instant::now() + Duration::from_secs(10);
        let mut socket =
            tungstenite::accept(transport::DeadlineStream::new(stream, overall)).unwrap();
        let durable = bridge_auth::durable_key(BOOTSTRAP);
        let hooks = PairingHooks {
            secret: &durable,
            bootstrap: BOOTSTRAP,
            pairing_generation: &|| None,
            consume_pairing: &|_| panic!("an idle peer never reaches admission"),
            failed_pairing: &|_| panic!("no reservation was taken"),
            origin_allowed: &|_| true,
            legacy_hint: &|| panic!("not legacy"),
            claim_session: &|| panic!("an idle peer never claims a session"),
            proof_phase: Duration::from_millis(200),
        };
        let started = Instant::now();
        let headers = tungstenite::http::HeaderMap::new();
        assert!(do_pairing(&mut socket, overall, &headers, &hooks).is_err());
        started.elapsed()
    });
    let stream = TcpStream::connect(address).unwrap();
    let (mut socket, _) = tungstenite::client(format!("ws://{address}"), stream).unwrap();
    for message in sent {
        socket
            .send(Message::Text(message.to_string().into()))
            .unwrap();
    }
    let held = server.join().unwrap();
    drop(socket);
    held
}

#[test]
fn an_idle_unauthenticated_peer_is_released_after_one_proof_phase() {
    let hello = json!({"type":"hello", "bridgeProtocol": bridge_auth::PROTOCOL, "clientNonce": nonce(), "hasSecret": true});
    for sent in [vec![], vec![hello]] {
        let held = hold_time_of_idle_peer(&sent);
        assert!(
            held < Duration::from_secs(2),
            "an idle peer after {} messages held its slot for {held:?}",
            sent.len()
        );
    }
}
