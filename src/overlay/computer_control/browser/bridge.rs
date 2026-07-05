//! Local WebSocket server that the SGT browser extension connects into, plus the
//! request/reply pump that carries CDP commands to it. One connection at a time;
//! the extension authenticates with HMAC challenge-response over a shared secret.

use std::collections::{HashMap, VecDeque};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Message, WebSocket, accept_hdr};

use super::super::telemetry::{self, Privacy};
use super::crypto;

const PORT_DEFAULT: u16 = 47800;
const REQ_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TICK: Duration = Duration::from_millis(50);
/// Most recent debugger events kept for `read_network` etc.
const EVENT_RING: usize = 400;

struct Outbound {
    json: Value,
    reply: mpsc::Sender<Value>,
}

struct Bridge {
    req_tx: Mutex<Option<mpsc::Sender<Outbound>>>,
    connected: AtomicBool,
    next_id: AtomicU64,
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

/// Load a persisted hex secret from `p`, or mint + store one on first use.
fn load_or_make(p: &std::path::Path) -> String {
    if let Ok(s) = std::fs::read_to_string(p) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let s = crypto::random_hex(24);
    let _ = std::fs::write(p, &s);
    s
}

fn bridge() -> &'static Bridge {
    BRIDGE.get_or_init(|| Bridge {
        req_tx: Mutex::new(None),
        connected: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        secret: load_or_make(&secret_path()),
        bootstrap: load_or_make(&bootstrap_path()),
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
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            telemetry::typed_error(
                "ERR_BROWSER_BRIDGE_BIND",
                "browser_bridge",
                "browser bridge failed to bind",
                json!({"addr": addr, "error": e.to_string()}),
            );
            return;
        }
    };
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
        *bridge().req_tx.lock().unwrap() = None;
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
    telemetry::human("cc-browser", "extension paired + connected");
    telemetry::event(
        "browser_extension_connected",
        "browser_bridge",
        Privacy::Safe,
        json!({"port": port()}),
    );
    bridge().connected.store(true, Ordering::SeqCst);
    super::record_connection(); // stamp the live moment - so a later nap reads as "reconnecting", not "set me up"
    let (tx, rx) = mpsc::channel::<Outbound>();
    *bridge().req_tx.lock().unwrap() = Some(tx);
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
    let mut pending: HashMap<u64, mpsc::Sender<Value>> = HashMap::new();
    loop {
        // Outbound first so a fresh request goes out without waiting a read tick.
        while let Ok(out) = rx.try_recv() {
            if let Some(id) = out.json.get("id").and_then(Value::as_u64) {
                pending.insert(id, out.reply);
            }
            send(ws, out.json)?;
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                    if let Some(id) = v.get("id").and_then(Value::as_u64) {
                        if let Some(reply) = pending.remove(&id) {
                            let _ = reply.send(v);
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
fn request(json_msg: Value) -> anyhow::Result<Value> {
    let tx = bridge().req_tx.lock().unwrap().clone();
    let Some(tx) = tx else {
        anyhow::bail!("browser extension not connected");
    };
    let (rtx, rrx) = mpsc::channel();
    tx.send(Outbound {
        json: json_msg,
        reply: rtx,
    })
    .map_err(|_| anyhow::anyhow!("bridge closed"))?;
    rrx.recv_timeout(REQ_TIMEOUT)
        .map_err(|_| anyhow::anyhow!("browser request timed out"))
}

/// Run a raw CDP command in the active tab's TOP frame and return its `result`.
pub(super) fn cdp(method: &str, params: Value) -> anyhow::Result<Value> {
    cdp_in(method, params, None)
}

/// Run a raw CDP command, optionally inside a specific (cross-origin) FRAME's CDP
/// session, and return its `result`. `None` = the active tab's top frame (== `cdp`).
/// The extension reads `sessionId` and routes via `chrome.debugger.sendCommand`
/// against that flat-attached child target - so out-of-process iframes (login /
/// payment / embed widgets) are reachable without any extension change.
pub(super) fn cdp_in(
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> anyhow::Result<Value> {
    let id = bridge().next_id.fetch_add(1, Ordering::SeqCst);
    let mut env = json!({"id": id, "type": "cdp", "method": method, "params": params});
    if let Some(sid) = session_id {
        env["sessionId"] = json!(sid);
    }
    let resp = request(env)?;
    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(resp.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        anyhow::bail!(
            "{}",
            resp.get("error")
                .and_then(Value::as_str)
                .unwrap_or("cdp error")
        )
    }
}

/// Send a non-CDP RPC envelope (e.g. `tabs`) the extension handles directly, and
/// return its `result`.
pub(super) fn rpc(type_: &str, mut extra: Value) -> anyhow::Result<Value> {
    let id = bridge().next_id.fetch_add(1, Ordering::SeqCst);
    extra["id"] = json!(id);
    extra["type"] = json!(type_);
    let resp = request(extra)?;
    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(resp.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        anyhow::bail!(
            "{}",
            resp.get("error")
                .and_then(Value::as_str)
                .unwrap_or("rpc error")
        )
    }
}

/// Snapshot the buffered debugger events whose CDP `method` contains `filter`.
pub(super) fn recent_events(filter: &str, limit: usize) -> Vec<Value> {
    let q = bridge().events.lock().unwrap();
    q.iter()
        .filter(|v| {
            filter.is_empty()
                || v.get("method")
                    .and_then(Value::as_str)
                    .map(|m| m.contains(filter))
                    .unwrap_or(false)
        })
        .rev()
        .take(limit)
        .cloned()
        .collect()
}
