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
use tungstenite::{Message, WebSocket, accept};

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
    secret: String,
    events: Mutex<VecDeque<Value>>,
    /// While `Some(deadline)` and now < deadline, an extension with no secret yet
    /// is auto-paired (the secret is handed to it over the socket). Opened by
    /// `browser_setup` so pairing needs no fragile in-browser popup.
    pairing_until: Mutex<Option<Instant>>,
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

/// Load the persisted pairing secret, or mint + store one on first use.
fn load_or_make_secret() -> String {
    let p = secret_path();
    if let Ok(s) = std::fs::read_to_string(&p) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let s = crypto::random_hex(24);
    let _ = std::fs::write(&p, &s);
    s
}

fn bridge() -> &'static Bridge {
    BRIDGE.get_or_init(|| Bridge {
        req_tx: Mutex::new(None),
        connected: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        secret: load_or_make_secret(),
        events: Mutex::new(VecDeque::new()),
        pairing_until: Mutex::new(None),
    })
}

/// Open a ~2-minute window during which a fresh (secret-less) extension is
/// auto-paired. Called by `browser_setup`.
pub(super) fn open_pairing_window() {
    *bridge().pairing_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(120));
}

fn in_pairing_window() -> bool {
    matches!(*bridge().pairing_until.lock().unwrap(), Some(t) if Instant::now() < t)
}

pub(crate) fn is_connected() -> bool {
    bridge().connected.load(Ordering::SeqCst)
}

/// The pairing code the user pastes into the extension popup (also the HMAC key).
pub(super) fn pairing_code() -> String {
    bridge().secret.clone()
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
            eprintln!("[cc-browser] cannot bind {addr}: {e}");
            return;
        }
    };
    eprintln!("[cc-browser] bridge listening on {addr}");
    for stream in listener.incoming().flatten() {
        if let Err(e) = handle_conn(stream) {
            eprintln!("[cc-browser] connection ended: {e}");
        }
        bridge().connected.store(false, Ordering::SeqCst);
        *bridge().req_tx.lock().unwrap() = None;
    }
}

fn handle_conn(stream: TcpStream) -> anyhow::Result<()> {
    stream.set_read_timeout(Some(READ_TICK))?;
    let mut ws = accept(stream).map_err(|e| anyhow::anyhow!("ws handshake: {e}"))?;
    if !do_pairing(&mut ws)? {
        let _ = ws.close(None);
        anyhow::bail!("pairing failed (bad code)");
    }
    eprintln!("[cc-browser] extension paired + connected");
    bridge().connected.store(true, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel::<Outbound>();
    *bridge().req_tx.lock().unwrap() = Some(tx);
    pump(&mut ws, rx)
}

fn is_transient(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io)
        if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// Read the next text frame of a given `type`, within the deadline.
fn wait_for_type(ws: &mut WebSocket<TcpStream>, deadline: Instant, ty: &str) -> anyhow::Result<Value> {
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
    // During a setup window, (RE)PAIR unconditionally with our current secret -
    // this must come FIRST so it also heals a STALE/mismatched secret left by a
    // prior install (otherwise such an extension loops on "bad code" forever).
    if in_pairing_window() {
        send(ws, json!({"type": "pair", "secret": bridge().secret}))?;
        *bridge().pairing_until.lock().unwrap() = None; // one extension per window
        eprintln!("[cc-browser] paired extension (pairing window)");
        return Ok(true);
    }
    // Outside a window: a paired extension proves it with HMAC challenge-response.
    if hello.get("hasSecret").and_then(Value::as_bool) == Some(true) {
        let nonce = crypto::random_hex(16);
        send(ws, json!({"type": "challenge", "nonce": nonce}))?;
        let auth = wait_for_type(ws, deadline, "auth")?;
        let mac = auth.get("mac").and_then(Value::as_str).unwrap_or("");
        let expect = crypto::hmac_sha256_hex(bridge().secret.as_bytes(), nonce.as_bytes());
        return Ok(crypto::ct_eq(mac.as_bytes(), expect.as_bytes()));
    }
    send(ws, json!({"type": "error", "error": "not pairing - run browser_setup first"}))?;
    Ok(false)
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
    tx.send(Outbound { json: json_msg, reply: rtx })
        .map_err(|_| anyhow::anyhow!("bridge closed"))?;
    rrx.recv_timeout(REQ_TIMEOUT)
        .map_err(|_| anyhow::anyhow!("browser request timed out"))
}

/// Run a raw CDP command in the active tab and return its `result` (or error).
pub(super) fn cdp(method: &str, params: Value) -> anyhow::Result<Value> {
    let id = bridge().next_id.fetch_add(1, Ordering::SeqCst);
    let resp = request(json!({"id": id, "type": "cdp", "method": method, "params": params}))?;
    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(resp.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        anyhow::bail!("{}", resp.get("error").and_then(Value::as_str).unwrap_or("cdp error"))
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
        anyhow::bail!("{}", resp.get("error").and_then(Value::as_str).unwrap_or("rpc error"))
    }
}

/// Snapshot the buffered debugger events whose CDP `method` contains `filter`.
pub(super) fn recent_events(filter: &str, limit: usize) -> Vec<Value> {
    let q = bridge().events.lock().unwrap();
    q.iter()
        .filter(|v| {
            filter.is_empty()
                || v.get("method").and_then(Value::as_str).map(|m| m.contains(filter)).unwrap_or(false)
        })
        .rev()
        .take(limit)
        .cloned()
        .collect()
}
