use super::*;
use std::sync::atomic::AtomicUsize;

#[test]
fn preauth_capacity_is_bounded_and_released() {
    let admission = Arc::new(Admission::default());
    let guards = (0..MAX_PREAUTH)
        .map(|_| admission.preauth().unwrap())
        .collect::<Vec<_>>();
    assert!(admission.preauth().is_none());
    drop(guards);
    assert!(admission.preauth().is_some());
}

#[test]
fn authenticated_owner_excludes_competitors_and_hints_until_cleanup() {
    let admission = Arc::new(Admission::default());
    let cleanups = Arc::new(AtomicUsize::new(0));
    let counter = cleanups.clone();
    let owner = admission
        .claim(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    assert!(admission.claim(|| panic!("competing cleanup")).is_none());
    admission.if_idle(|| panic!("unverified hint during active session"));
    drop(owner);
    assert_eq!(cleanups.load(Ordering::SeqCst), 1);
    admission.if_idle(|| {
        cleanups.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(cleanups.load(Ordering::SeqCst), 2);
    assert!(admission.claim(|| {}).is_some());
}

fn client_frame(key: &str, client: &str, server: &str) -> Value {
    let domain = "sgt-browser-v8";
    let key_hex = crypto::hmac_sha256_hex(
        key.as_bytes(),
        format!("{domain}|aead|{client}|{server}").as_bytes(),
    );
    let key = crate::crypto::decode_hex(&key_hex, "test key").unwrap();
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&1u32.to_be_bytes());
    nonce[4..].copy_from_slice(&1u64.to_be_bytes());
    let aad = format!("{domain}|frame|c2s|{client}|{server}|1");
    let bytes = super::super::super::bridge_cipher::crypt(
        &key,
        &nonce,
        aad.as_bytes(),
        br#"{"type":"ready_ack"}"#,
        true,
    )
    .unwrap();
    let ciphertext: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    json!({"type":"secure", "seq":1, "ciphertext":ciphertext})
}

fn run_pairing(bootstrap: bool, supersede: bool, reopen_after_pair: bool, fail_ack: bool) {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let admission = Arc::new(Admission::default());
    let owner_admission = admission.clone();
    let generation = Arc::new(AtomicU64::new(1));
    let owner_generation = generation.clone();
    let failures = Arc::new(AtomicU64::new(0));
    let owner_failures = failures.clone();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut socket =
            tungstenite::accept(transport::DeadlineStream::new(stream, deadline)).unwrap();
        let secret = super::super::super::bridge_auth::durable_key("fixture bootstrap");
        let claim = || owner_admission.claim(|| {});
        let hooks = PairingHooks {
            secret: &secret,
            bootstrap: "fixture bootstrap",
            pairing_generation: &|| {
                let generation = owner_generation.load(Ordering::SeqCst);
                (bootstrap && generation != 0).then_some(generation)
            },
            consume_pairing: &|generation| {
                owner_generation
                    .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            },
            failed_pairing: &|generation| {
                owner_failures.store(generation, Ordering::SeqCst);
            },
            origin_allowed: &|_| true,
            legacy_hint: &|| panic!("not legacy"),
            claim_session: &claim,
            proof_phase: Duration::from_secs(3),
        };
        let result = do_pairing(
            &mut socket,
            deadline,
            &tungstenite::http::HeaderMap::new(),
            &hooks,
        );
        if fail_ack {
            assert!(result.is_err());
            assert_eq!(owner_failures.load(Ordering::SeqCst), 1);
            assert!(owner_admission.claim(|| {}).is_some());
            return;
        }
        let result = result.unwrap();
        if supersede {
            assert!(result.is_none());
            assert!(owner_admission.claim(|| {}).is_some());
            return;
        }
        let AuthenticatedPair {
            mut session,
            owner,
            metadata,
        } = result.unwrap();
        assert_eq!(
            owner_generation.load(Ordering::SeqCst),
            if reopen_after_pair {
                2
            } else if bootstrap {
                0
            } else {
                1
            }
        );
        assert_eq!(metadata.protocol, 8);
        assert!(session.seal(&json!({"type":"test"})).is_ok());
        assert!(owner_admission.claim(|| {}).is_none());
        drop(owner);
        assert!(owner_admission.claim(|| {}).is_some());
    });
    let stream = TcpStream::connect(address).unwrap();
    let (mut socket, _) = tungstenite::client(format!("ws://{address}"), stream).unwrap();
    let client = "11".repeat(32);
    socket.send(Message::Text(json!({"type":"hello", "bridgeProtocol":8, "clientNonce":client, "hasBootstrap":bootstrap, "hasSecret":!bootstrap}).to_string().into())).unwrap();
    let challenge: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    let nonce = challenge["nonce"].as_str().unwrap();
    let kind = if bootstrap { "bootstrap" } else { "secret" };
    let key = if bootstrap {
        super::super::super::bridge_auth::bootstrap_key("fixture bootstrap")
    } else {
        super::super::super::bridge_auth::durable_key("fixture bootstrap")
    };
    assert_eq!(
        challenge["mac"],
        super::super::super::bridge_auth::proof(&key, "server", kind, &client, nonce)
    );
    let proof = super::super::super::bridge_auth::proof(&key, "client", kind, &client, nonce);
    if supersede {
        generation.store(2, Ordering::SeqCst);
    }
    socket
        .send(Message::Text(
            json!({"type":"auth", "mac":proof}).to_string().into(),
        ))
        .unwrap();
    if supersede {
        server.join().unwrap();
        return;
    }
    if bootstrap {
        assert_eq!(
            serde_json::from_str::<Value>(socket.read().unwrap().to_text().unwrap()).unwrap()["type"],
            "secure"
        );
        if fail_ack {
            let _ = socket.close(None);
            server.join().unwrap();
            return;
        }
        if reopen_after_pair {
            generation.store(2, Ordering::SeqCst);
        }
    }
    assert_eq!(
        serde_json::from_str::<Value>(socket.read().unwrap().to_text().unwrap()).unwrap()["type"],
        "secure"
    );
    socket
        .send(Message::Text(
            client_frame(&key, &client, nonce).to_string().into(),
        ))
        .unwrap();
    server.join().unwrap();
}

#[test]
fn isolated_loopback_bootstrap_and_durable_reconnect_complete_authentication() {
    run_pairing(true, false, false, false);
    run_pairing(false, false, false, false);
}

#[test]
fn superseded_bootstrap_challenge_cannot_claim_a_session() {
    run_pairing(true, true, false, false);
}

#[test]
fn reopening_after_admitted_pair_does_not_consume_the_new_window() {
    run_pairing(true, false, true, false);
}

#[test]
fn delivered_pair_without_ack_marks_generation_for_fresh_setup() {
    run_pairing(true, false, false, true);
}

#[test]
fn old_pairing_completion_cannot_consume_a_new_window() {
    let now = Instant::now();
    let mut window = PairingWindow::default();
    window.open(now + Duration::from_secs(10));
    let old = window.current(now).unwrap();
    window.open(now + Duration::from_secs(20));
    assert!(!window.consume(old, now));
    let current = window.current(now).unwrap();
    assert_ne!(current, old);
    assert!(window.consume(current, now));
    assert_eq!(window.current(now), None);
    window.failed_after_admission(current);
    assert!(window.retry_required);
    window.authenticated();
    assert!(!window.retry_required);
    window.failed_after_admission(current);
    window.open(now + Duration::from_secs(10));
    assert!(!window.retry_required);
    window.failed_after_admission(current);
    assert!(!window.retry_required);
    window.open(now);
    assert_eq!(window.current(now), None);
}

#[test]
fn failed_admitted_pairing_requests_fresh_setup_without_restoring_window() {
    let now = Instant::now();
    let window = std::cell::RefCell::new(PairingWindow::default());
    window.borrow_mut().open(now + Duration::from_secs(10));
    let generation = window.borrow().current(now).unwrap();
    assert!(window.borrow_mut().consume(generation, now));
    {
        let mark_failed = |generation| window.borrow_mut().failed_after_admission(generation);
        let _reservation = PairingReservation {
            generation,
            failure: &mark_failed,
            completed: false,
        };
    }
    assert!(window.borrow().retry_required);
    assert_eq!(window.borrow().current(now), None);
}
