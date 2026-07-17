//! Local WebSocket server that the SGT browser extension connects into, plus the
//! request/reply pump that carries CDP commands to it. One connection at a time;
//! the extension authenticates with HMAC challenge-response over a shared secret.

use std::collections::{HashMap, VecDeque};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Message, WebSocket, accept_hdr};

use super::super::telemetry::{self, Privacy};
pub(super) use super::bridge_cdp::{cdp, cdp_in, cdp_in_active_tab, cdp_in_tab, cdp_on_tab};
pub(super) use super::bridge_rpc::rpc;
use super::crypto;

const PORT_DEFAULT: u16 = 47800;
const REQ_TIMEOUT: Duration = Duration::from_secs(15);
const CLEANUP_REQ_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TICK: Duration = Duration::from_millis(50);
/// Most recent debugger events kept for `read_network` etc.
const EVENT_RING: usize = 400;

struct Outbound {
    json: Value,
    reply: mpsc::Sender<Value>,
    cancel: Option<Arc<AtomicBool>>,
    deadline: Instant,
}

#[derive(Clone)]
struct ConnectionHandle {
    epoch: u64,
    sender: mpsc::Sender<Outbound>,
}

struct Bridge {
    connection: Mutex<Option<ConnectionHandle>>,
    connected: AtomicBool,
    next_id: AtomicU64,
    next_connection_epoch: AtomicU64,
    /// The durable HMAC key. Handed to the extension ONCE (after it proves the
    /// bootstrap secret), then used for challenge-response on every reconnect.
    secret: String,
    /// Per-install bootstrap key, also stamped into the extension's own files
    /// (`bootstrap.js`). On the first connect the extension proves knowledge of it,
    /// which is what authorizes handing over `secret`. A random local socket client
    /// that can't read the extension's files cannot prove it - closing the
    /// "anyone-on-localhost grabs the secret during the window" race.
    bootstrap: String,
    events: Mutex<VecDeque<Value>>,
    /// While `Some(deadline)` and now < deadline, a freshly-written extension that
    /// proves the bootstrap secret is handed the durable secret. Opened by
    /// `browser_setup` so pairing needs no fragile in-browser popup.
    pairing_until: Mutex<Option<Instant>>,
    /// The extension's id, learned (trust-on-first-use) from the first successful
    /// pairing. Used to pin a chrome-extension:// Origin on later handshakes.
    ext_id: Mutex<Option<String>>,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();

fn port() -> u16 {
    std::env::var("CC_BROWSER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(PORT_DEFAULT)
}

fn secret_path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_browser_secret.txt")
}

fn bootstrap_path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_browser_bootstrap.txt")
}

fn writable_secret_path(name: &str) -> std::path::PathBuf {
    crate::paths::app_runtime_config_dir().join(name)
}

/// Load existing pairing material without touching it. A missing value is
/// minted only into the active runtime's writable state root.
fn load_or_make(read_path: &std::path::Path, write_path: &std::path::Path) -> String {
    if let Ok(s) = std::fs::read_to_string(read_path) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let s = crypto::random_hex(24);
    if let Some(parent) = write_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(write_path, &s);
    s
}

fn bridge() -> &'static Bridge {
    BRIDGE.get_or_init(|| Bridge {
        connection: Mutex::new(None),
        connected: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        next_connection_epoch: AtomicU64::new(0),
        secret: load_or_make(
            &secret_path(),
            &writable_secret_path("cc_browser_secret.txt"),
        ),
        bootstrap: load_or_make(
            &bootstrap_path(),
            &writable_secret_path("cc_browser_bootstrap.txt"),
        ),
        events: Mutex::new(VecDeque::new()),
        pairing_until: Mutex::new(None),
        ext_id: Mutex::new(None),
    })
}

/// Open a 10-minute window during which a fresh-or-stale extension is auto-paired
/// (with our current secret). Generous because installing the unpacked extension
/// can be slow (vision waits, user chatter) - a short window expired before the
/// extension finished loading, leaving a stale secret looping on "bad code".
pub(super) fn open_pairing_window() {
    *bridge().pairing_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(600));
}

fn in_pairing_window() -> bool {
    matches!(*bridge().pairing_until.lock().unwrap(), Some(t) if Instant::now() < t)
}

/// Whether a setup pairing window is currently open (surfaced in `browser_status`).
pub(super) fn pairing_window_open() -> bool {
    in_pairing_window()
}

pub(crate) fn is_connected() -> bool {
    bridge().connected.load(Ordering::SeqCst)
}

/// The per-install bootstrap key to stamp into the extension's files so it can
/// prove itself on first connect. Not the durable secret (which we never expose).
pub(super) fn bootstrap_secret() -> String {
    bridge().bootstrap.clone()
}

pub(super) fn port_for_display() -> u16 {
    port()
}

/// Start the local server once (idempotent). The extension connects in.
pub(crate) fn ensure_started() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(listen_loop);
    });
}

fn listen_loop() {
    let addr = format!("127.0.0.1:{}", port());
    let listener = super::bridge_listener::bind_with_retry(&addr);
    telemetry::human("cc-browser", format!("bridge listening on {addr}"));
    telemetry::event(
        "browser_bridge_listening",
        "browser_bridge",
        Privacy::Safe,
        json!({"addr": addr}),
    );
    for stream in listener.incoming().flatten() {
        if let Err(e) = handle_conn(stream) {
            telemetry::event(
                "browser_bridge_connection_ended",
                "browser_bridge",
                Privacy::Safe,
                json!({"error": e.to_string()}),
            );
            telemetry::human("cc-browser", format!("connection ended: {e}"));
        }
        // Only stamp the last-live moment if we were actually paired (not a failed probe),
        // so `recently_connected()` tracks real connectivity, not random socket attempts.
        if bridge().connected.swap(false, Ordering::SeqCst) {
            super::record_connection();
        }
        super::capabilities::reset();
        *bridge().connection.lock().unwrap() = None;
    }
}

fn handle_conn(stream: TcpStream) -> anyhow::Result<()> {
    stream.set_read_timeout(Some(READ_TICK))?;
    // Origin gate (defense-in-depth; the bootstrap / durable proof is the real one):
    //  - http(s) web-page Origin → REJECT. A browser sets Origin to the page URL and
    //    a page cannot forge it, so this slams the door on a malicious page hitting
    //    our localhost port.
    //  - chrome-extension:// Origin → accept, but once we've learned OUR extension's
    //    id (trust-on-first-use) pin it - except inside a setup window, where the
    //    bootstrap proof gates and the id may legitimately have changed.
    //  - no Origin (an MV3 service-worker WS may omit it) → allow the socket open;
    //    it still gets nothing without the proof.
    let mut ws = accept_hdr(stream, |req: &Request, resp: Response| {
        let forbidden =
            |msg: &str| -> Result<Response, tungstenite::http::Response<Option<String>>> {
                Err(tungstenite::http::Response::builder()
                    .status(tungstenite::http::StatusCode::FORBIDDEN)
                    .body(Some(msg.to_string()))
                    .unwrap())
            };
        match req.headers().get("origin").and_then(|v| v.to_str().ok()) {
            Some(o) if o.starts_with("http://") || o.starts_with("https://") => {
                forbidden("web pages may not connect to the SGT bridge")
            }
            Some(o) if o.starts_with("chrome-extension://") => {
                let id = o
                    .trim_start_matches("chrome-extension://")
                    .trim_end_matches('/');
                match (in_pairing_window(), bridge().ext_id.lock().unwrap().clone()) {
                    (false, Some(exp)) if id != exp => forbidden("unexpected extension id"),
                    _ => Ok(resp),
                }
            }
            _ => Ok(resp),
        }
    })
    .map_err(|e| anyhow::anyhow!("ws handshake: {e}"))?;
    if !do_pairing(&mut ws)? {
        let _ = ws.close(None);
        anyhow::bail!("pairing failed (bad code)");
    }
    let (tx, rx) = mpsc::channel::<Outbound>();
    let epoch = bridge()
        .next_connection_epoch
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    *bridge().connection.lock().unwrap() = Some(ConnectionHandle { epoch, sender: tx });
    telemetry::human("cc-browser", "extension paired + connected");
    telemetry::event(
        "browser_extension_connected",
        "browser_bridge",
        Privacy::Safe,
        json!({
            "port": port(),
            "protocol_version": super::capabilities::protocol_version(),
            "capabilities": super::capabilities::list(),
            "connection_epoch": epoch,
        }),
    );
    bridge().connected.store(true, Ordering::SeqCst);
    super::record_connection(); // stamp the live moment - so a later nap reads as "reconnecting", not "set me up"
    pump(&mut ws, rx)
}

fn is_transient(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io)
        if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// Read the next text frame of a given `type`, within the deadline.
fn wait_for_type(
    ws: &mut WebSocket<TcpStream>,
    deadline: Instant,
    ty: &str,
) -> anyhow::Result<Value> {
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for '{ty}'");
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&t)
                    && v.get("type").and_then(Value::as_str) == Some(ty)
                {
                    return Ok(v);
                }
            }
            Ok(Message::Close(_)) => anyhow::bail!("closed during pairing"),
            Ok(_) => {}
            Err(e) if is_transient(&e) => continue,
            Err(e) => anyhow::bail!("read: {e}"),
        }
    }
}

fn do_pairing(ws: &mut WebSocket<TcpStream>) -> anyhow::Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let hello = wait_for_type(ws, deadline, "hello")?;
    let protocol = hello
        .get("bridgeProtocol")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let advertised_capabilities = hello.get("capabilities").cloned();
    let ext_id = hello
        .get("extId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let has_secret = hello.get("hasSecret").and_then(Value::as_bool) == Some(true);
    let has_bootstrap = hello.get("hasBootstrap").and_then(Value::as_bool) == Some(true);

    // FIRST pairing (inside a setup window): the freshly-written extension proves the
    // per-install bootstrap secret - which only IT can read from its own files - and
    // we hand back the durable secret. This must come FIRST so it also HEALS a
    // stale/mismatched durable secret (re-hands the current one), and the proof is
    // REQUIRED so a random local socket client that can't read the extension files
    // gets nothing (closes the open-pairing-window race).
    if in_pairing_window() && has_bootstrap {
        let nonce = crypto::random_hex(16);
        send(
            ws,
            json!({"type": "challenge", "nonce": nonce, "use": "bootstrap"}),
        )?;
        let auth = wait_for_type(ws, deadline, "auth")?;
        let mac = auth.get("mac").and_then(Value::as_str).unwrap_or("");
        let expect = crypto::hmac_sha256_hex(bridge().bootstrap.as_bytes(), nonce.as_bytes());
        if crypto::ct_eq(mac.as_bytes(), expect.as_bytes()) {
            send(ws, json!({"type": "pair", "secret": bridge().secret}))?;
            *bridge().pairing_until.lock().unwrap() = None; // one extension per window
            commit_ext_id(ext_id);
            super::capabilities::negotiate(protocol, advertised_capabilities.as_ref());
            telemetry::human("cc-browser", "paired extension (bootstrap proof)");
            telemetry::event(
                "browser_extension_paired",
                "browser_bridge",
                Privacy::Safe,
                json!({"proof": "bootstrap"}),
            );
            return Ok(true);
        }
        // Bad proof: leave the window OPEN for the real extension, drop this socket.
        let _ = send(
            ws,
            json!({"type": "error", "error": "bootstrap proof failed"}),
        );
        telemetry::typed_error(
            "ERR_BROWSER_BOOTSTRAP_PROOF",
            "browser_bridge",
            "rejected browser bridge connection with bad bootstrap proof",
            json!({}),
        );
        return Ok(false);
    }

    // RECONNECT: an established extension proves the durable secret with HMAC
    // challenge-response (the bootstrap secret is never used again after pairing).
    if has_secret {
        let nonce = crypto::random_hex(16);
        send(
            ws,
            json!({"type": "challenge", "nonce": nonce, "use": "secret"}),
        )?;
        let auth = wait_for_type(ws, deadline, "auth")?;
        let mac = auth.get("mac").and_then(Value::as_str).unwrap_or("");
        let expect = crypto::hmac_sha256_hex(bridge().secret.as_bytes(), nonce.as_bytes());
        if crypto::ct_eq(mac.as_bytes(), expect.as_bytes()) {
            commit_ext_id(ext_id);
            super::capabilities::negotiate(protocol, advertised_capabilities.as_ref());
            return Ok(true);
        }
        return Ok(false);
    }

    send(
        ws,
        json!({"type": "error", "error": "not paired - run browser_setup first"}),
    )?;
    Ok(false)
}

/// Remember the extension's id (trust-on-first-use) for the Origin pin.
fn commit_ext_id(id: Option<String>) {
    if let Some(id) = id {
        *bridge().ext_id.lock().unwrap() = Some(id);
    }
}

fn send(ws: &mut WebSocket<TcpStream>, v: Value) -> anyhow::Result<()> {
    ws.write(Message::Text(v.to_string().into()))?;
    ws.flush()?;
    Ok(())
}

fn record_event(v: Value) {
    let mut q = bridge().events.lock().unwrap();
    if q.len() >= EVENT_RING {
        q.pop_front();
    }
    q.push_back(v);
}

/// The single-owner message pump: drains outbound CDP requests onto the socket and
/// routes inbound replies back to their waiting caller by id; buffers events.
fn pump(ws: &mut WebSocket<TcpStream>, rx: mpsc::Receiver<Outbound>) -> anyhow::Result<()> {
    let mut pending: HashMap<u64, super::bridge_wait::PendingReply> = HashMap::new();
    loop {
        super::bridge_wait::prune_inactive(&mut pending, Instant::now());
        // Outbound first so a fresh request goes out without waiting a read tick.
        while let Ok(out) = rx.try_recv() {
            if Instant::now() >= out.deadline
                || out
                    .cancel
                    .as_deref()
                    .is_some_and(|token| token.load(Ordering::SeqCst))
            {
                continue;
            }
            if let Some(id) = out.json.get("id").and_then(Value::as_u64) {
                pending.insert(
                    id,
                    super::bridge_wait::PendingReply::new(out.reply, out.cancel, out.deadline),
                );
            }
            send(ws, out.json)?;
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                    if let Some(id) = v.get("id").and_then(Value::as_u64) {
                        if let Some(reply) = pending.remove(&id) {
                            reply.deliver(v);
                        }
                    } else if v.get("type").and_then(Value::as_str) == Some("event") {
                        record_event(v);
                    }
                }
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(e) if is_transient(&e) => {}
            Err(e) => anyhow::bail!("read: {e}"),
        }
    }
}

/// Send one envelope and block for its correlated reply.
pub(super) fn request(json_msg: Value) -> anyhow::Result<Value> {
    let cancel = super::readiness::current_cancel();
    let timeout = super::readiness::bounded_request_timeout(REQ_TIMEOUT);
    if timeout.is_zero() {
        anyhow::bail!("browser request deadline elapsed before dispatch");
    }
    request_with(json_msg, cancel, timeout, None)
}

/// Dispatch bounded compensating cleanup after an input edge. Cleanup must not
/// inherit user cancellation: its only purpose is to release already-held input.
pub(super) fn request_cleanup(json_msg: Value) -> anyhow::Result<Value> {
    request_with(json_msg, None, CLEANUP_REQ_TIMEOUT, None)
}

pub(super) fn request_on_epoch(json_msg: Value, epoch: u64) -> anyhow::Result<Value> {
    let cancel = super::readiness::current_cancel();
    let timeout = super::readiness::bounded_request_timeout(REQ_TIMEOUT);
    request_with(json_msg, cancel, timeout, Some(epoch))
}

pub(super) fn request_cleanup_until(
    json_msg: Value,
    epoch: u64,
    deadline: Instant,
) -> anyhow::Result<Value> {
    let timeout = deadline
        .saturating_duration_since(Instant::now())
        .min(CLEANUP_REQ_TIMEOUT);
    request_with(json_msg, None, timeout, Some(epoch))
}

fn request_with(
    json_msg: Value,
    cancel: Option<Arc<AtomicBool>>,
    timeout: Duration,
    expected_epoch: Option<u64>,
) -> anyhow::Result<Value> {
    super::bridge_wait::ensure_dispatch_allowed(cancel.as_deref())?;
    if timeout.is_zero() {
        return Err(super::bridge_wait::unavailable_before_dispatch());
    }
    let connection = bridge().connection.lock().unwrap().clone();
    let Some(connection) = connection else {
        return Err(super::bridge_wait::unavailable_before_dispatch());
    };
    if !is_connected()
        || expected_epoch.is_some_and(|epoch| epoch == 0 || epoch != connection.epoch)
    {
        return Err(super::bridge_wait::unavailable_before_dispatch());
    }
    let (rtx, rrx) = mpsc::channel();
    let deadline = Instant::now() + timeout;
    connection
        .sender
        .send(Outbound {
            json: json_msg,
            reply: rtx,
            cancel: cancel.clone(),
            deadline,
        })
        .map_err(|_| super::bridge_wait::unavailable_before_dispatch())?;
    let response = super::bridge_wait::receive(&rrx, deadline, cancel.as_deref())?;
    if expected_epoch.is_some_and(|epoch| !connection_matches(epoch)) {
        return Err(super::bridge_wait::closed_after_dispatch());
    }
    Ok(response)
}

fn connection_matches(epoch: u64) -> bool {
    epoch != 0
        && bridge()
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|connection| connection.epoch == epoch)
}

pub(super) fn connection_epoch() -> u64 {
    bridge()
        .connection
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0, |connection| connection.epoch)
}

pub(super) fn next_request_id() -> u64 {
    bridge().next_id.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn response_error(response: &Value, fallback: &str) -> anyhow::Error {
    if response.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_CAPABILITY_UNSUPPORTED") {
        return super::capabilities::unsupported(
            response
                .get("capability")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        );
    }
    anyhow::anyhow!(
        "{}",
        response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or(fallback)
    )
}

pub(super) fn recent_events_on_tab(filter: &str, limit: usize, tab_id: Option<i64>) -> Vec<Value> {
    let q = bridge().events.lock().unwrap();
    q.iter()
        .filter(|event| event_matches(event, filter, tab_id))
        .rev()
        .take(limit)
        .cloned()
        .collect()
}

fn event_matches(event: &Value, filter: &str, tab_id: Option<i64>) -> bool {
    let method_matches = filter.is_empty()
        || event
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| method.contains(filter));
    let tab_matches = tab_id.is_none() || event.get("tabId").and_then(Value::as_i64) == tab_id;
    method_matches && tab_matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_matches_method_and_exact_root_tab() {
        let event = json!({"method": "Network.responseReceived", "tabId": 31});
        assert!(event_matches(&event, "response", Some(31)));
        assert!(!event_matches(&event, "response", Some(32)));
        assert!(!event_matches(&event, "console", Some(31)));
        assert!(event_matches(&event, "", None));
    }
}
