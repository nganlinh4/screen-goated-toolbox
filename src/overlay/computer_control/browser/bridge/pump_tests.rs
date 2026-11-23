use super::*;
use std::net::TcpListener;

const DOMAIN: &str = "sgt-browser-v8";

/// The extension's side of an authenticated session, written independently of
/// the host's `Session` so the two must agree on the wire format.
struct Peer {
    key: Vec<u8>,
    client: String,
    server: String,
    sent: u64,
    received: u64,
}

impl Peer {
    fn new(key: &str, client: &str, server: &str) -> Self {
        let key = crypto::hmac_sha256_hex(
            key.as_bytes(),
            format!("{DOMAIN}|aead|{client}|{server}").as_bytes(),
        );
        Self {
            key: crate::crypto::decode_hex(&key, "test key").unwrap(),
            client: client.into(),
            server: server.into(),
            sent: 0,
            received: 0,
        }
    }

    fn crypt(&self, direction: &str, seq: u64, data: &[u8], encrypt: bool) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&u32::from(direction == "c2s").to_be_bytes());
        nonce[4..].copy_from_slice(&seq.to_be_bytes());
        let aad = format!(
            "{DOMAIN}|frame|{direction}|{}|{}|{seq}",
            self.client, self.server
        );
        super::super::super::bridge_cipher::crypt(&self.key, &nonce, aad.as_bytes(), data, encrypt)
            .unwrap()
    }

    fn seal(&mut self, body: &Value) -> Message {
        self.sent += 1;
        let bytes = self.crypt("c2s", self.sent, body.to_string().as_bytes(), true);
        let ciphertext: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Message::Text(
            json!({"type":"secure", "seq":self.sent, "ciphertext":ciphertext})
                .to_string()
                .into(),
        )
    }

    fn open(&mut self, message: Message) -> Value {
        let frame: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        self.received += 1;
        assert_eq!(frame["seq"], self.received);
        let ciphertext =
            crate::crypto::decode_hex(frame["ciphertext"].as_str().unwrap(), "frame").unwrap();
        serde_json::from_slice(&self.crypt("s2c", self.received, &ciphertext, false)).unwrap()
    }
}

struct Harness {
    sender: mpsc::Sender<PumpInput>,
    peer: Peer,
    client: tungstenite::WebSocket<TcpStream>,
    pump: std::thread::JoinHandle<anyhow::Result<()>>,
}

fn start() -> Harness {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel::<PumpInput>();
    let wake = tx.clone();
    let (client_nonce, server_nonce) = ("11".repeat(32), "22".repeat(32));
    let (host_client, host_server) = (client_nonce.clone(), server_nonce.clone());
    let pump = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut ws = tungstenite::accept(transport::DeadlineStream::new(stream, deadline)).unwrap();
        ws.get_mut().authenticated(wake).unwrap();
        let session =
            super::super::super::bridge_auth::Session::new("key", &host_client, &host_server);
        super::run(&mut ws, rx, session, 1)
    });
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (client, _) = tungstenite::client(format!("ws://{address}"), stream).unwrap();
    Harness {
        sender: tx,
        peer: Peer::new("key", &client_nonce, &server_nonce),
        client,
        pump,
    }
}

fn request(harness: &Harness, body: Value) -> mpsc::Receiver<Value> {
    let (reply, receiver) = mpsc::channel();
    harness
        .sender
        .send(PumpInput::Request(Outbound {
            json: body,
            reply,
            cancel: None,
            deadline: Instant::now() + Duration::from_secs(5),
        }))
        .unwrap();
    receiver
}

#[test]
fn idle_pump_dispatches_each_request_without_waiting_for_a_sweep_tick() {
    let mut harness = start();
    const ROUND_TRIPS: u64 = 10;
    let started = Instant::now();
    for id in 1..=ROUND_TRIPS {
        let reply = request(&harness, json!({"id": id, "type": "frames", "tabId": 3}));
        let sent = harness.peer.open(harness.client.read().unwrap());
        assert_eq!(sent["id"], id);
        assert!(sent.get("effectToken").is_none());
        let answer = harness.peer.seal(&json!({"id": id, "ok": true}));
        harness.client.send(answer).unwrap();
        assert_eq!(
            reply.recv_timeout(Duration::from_secs(2)).unwrap()["id"],
            id
        );
    }
    let elapsed = started.elapsed();
    // A pump that only sends between timed socket reads idles one full tick
    // per request here, because nothing else arrives on the socket.
    assert!(
        elapsed < PENDING_SWEEP_TICK * (ROUND_TRIPS as u32) / 2,
        "{ROUND_TRIPS} idle round trips took {elapsed:?}"
    );
    harness.client.close(None).unwrap();
    let _ = harness.client.flush();
    assert!(harness.pump.join().unwrap().is_ok());
}

#[test]
fn replies_are_correlated_by_id_and_unknown_ids_are_ignored() {
    let mut harness = start();
    let first = request(&harness, json!({"id": 41, "type": "cdp", "method": "A"}));
    let second = request(&harness, json!({"id": 42, "type": "cdp", "method": "B"}));
    let sent_first = harness.peer.open(harness.client.read().unwrap());
    let sent_second = harness.peer.open(harness.client.read().unwrap());
    assert_eq!(
        (sent_first["id"].clone(), sent_second["id"].clone()),
        (json!(41), json!(42))
    );
    for sent in [&sent_first, &sent_second] {
        assert!(
            sent["timeoutMs"]
                .as_u64()
                .is_some_and(|ms| ms > 0 && ms <= 15_000)
        );
        assert!(valid_effect_token(sent.get("effectToken")).is_some());
    }
    assert_ne!(sent_first["effectToken"], sent_second["effectToken"]);
    for answer in [
        json!({"id": 99, "ok": true, "result": "stray"}),
        json!({"id": 42, "ok": true, "result": "second"}),
        json!({"id": 41, "ok": true, "result": "first"}),
    ] {
        let frame = harness.peer.seal(&answer);
        harness.client.send(frame).unwrap();
    }
    let timeout = Duration::from_secs(2);
    assert_eq!(second.recv_timeout(timeout).unwrap()["result"], "second");
    assert_eq!(first.recv_timeout(timeout).unwrap()["result"], "first");
    assert!(second.recv_timeout(Duration::from_millis(100)).is_err());
    harness.client.close(None).unwrap();
    let _ = harness.client.flush();
    assert!(harness.pump.join().unwrap().is_ok());
}

#[test]
fn a_tampered_frame_ends_the_pump() {
    let mut harness = start();
    let mut forged = Peer::new("other key", &"11".repeat(32), &"22".repeat(32));
    let frame = forged.seal(&json!({"id": 1, "ok": true}));
    harness.client.send(frame).unwrap();
    assert!(harness.pump.join().unwrap().is_err());
}

#[test]
fn an_effect_in_flight_when_the_connection_ends_is_reported_as_uncertain() {
    let mut harness = start();
    let reply = request(
        &harness,
        json!({"id": 51, "type": "cdp", "method": "Input.dispatchKeyEvent"}),
    );
    let sent = harness.peer.open(harness.client.read().unwrap());
    assert_eq!(sent["id"], 51);
    harness.client.close(None).unwrap();
    let _ = harness.client.flush();
    assert!(harness.pump.join().unwrap().is_ok());
    let error = bridge_wait::receive(&reply, Instant::now() + Duration::from_secs(2), None)
        .expect_err("a dispatched request without a reply is never a success");
    assert_eq!(bridge_wait::dispatch_effect(&error), Some(true));
}
