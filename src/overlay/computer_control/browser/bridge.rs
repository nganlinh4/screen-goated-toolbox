//! Local WebSocket server that the SGT browser extension connects into, plus the
//! request/reply pump that carries CDP commands to it. One connection at a time;
//! peers mutually authenticate and protect ordered messages with AES-GCM.

use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Message, WebSocket};

use super::super::telemetry::{self, Privacy};
pub(super) use super::bridge_cdp::{cdp, cdp_in, cdp_in_active_tab, cdp_in_tab, cdp_on_tab};
pub(super) use super::bridge_rpc::rpc;
use super::crypto;
mod admission;
mod events;
mod inbound;
mod keys;
mod log_throttle;
mod origin;
mod pairing;
#[cfg(test)]
use pairing::PairingReservation;
use pairing::{AuthenticatedPair, PairingHooks, do_pairing};
mod pump;
mod secret;
pub(super) use secret::Error as PairingKeyError;
mod server;
mod transport;
pub(super) use events::recent_events_on_tab;
#[cfg(test)]
pub(super) use events::record_for_test;
type BridgeSocket = WebSocket<transport::DeadlineStream>;

const PORT_DEFAULT: u16 = 47800;
const REQ_TIMEOUT: Duration = Duration::from_secs(15);
const CLEANUP_REQ_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the pump sweeps expired or cancelled pending replies. Requests and
/// inbound frames wake the pump immediately and never wait for this tick.
const PENDING_SWEEP_TICK: Duration = Duration::from_millis(50);
/// Most recent debugger events kept for `read_network` etc.
const EVENT_RING: usize = 400;

struct Outbound {
    json: Value,
    reply: mpsc::Sender<Value>,
    cancel: Option<Arc<AtomicBool>>,
    deadline: Instant,
}

/// Everything that wakes the pump: a request to send or bytes from the socket.
enum PumpInput {
    Request(Outbound),
    Inbound(inbound::Inbound),
}

impl From<inbound::Inbound> for PumpInput {
    fn from(inbound: inbound::Inbound) -> Self {
        Self::Inbound(inbound)
    }
}

#[derive(Clone)]
struct ConnectionHandle {
    epoch: u64,
    sender: mpsc::Sender<PumpInput>,
}

struct Bridge {
    connection: Mutex<Option<ConnectionHandle>>,
    connected: AtomicBool,
    legacy_update_required: AtomicBool,
    next_id: AtomicU64,
    next_connection_epoch: AtomicU64,
    keys: keys::KeyStore,
    events: Mutex<VecDeque<Value>>,
    /// While `Some(deadline)` and now < deadline, a freshly-written extension that
    /// proves the bootstrap secret derives its durable key. Opened by
    /// `browser_setup` so pairing needs no fragile in-browser popup.
    pairing_window: Mutex<PairingWindow>,
    /// The extension's id, learned (trust-on-first-use) from the first successful
    /// pairing. Used to pin a chrome-extension:// Origin on later handshakes.
    ext_id: Mutex<Option<String>>,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();

#[derive(Default)]
struct PairingWindow {
    generation: u64,
    until: Option<Instant>,
    retry_required: bool,
}

impl PairingWindow {
    fn open(&mut self, until: Instant) {
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
            self.until = Some(until);
            self.retry_required = false;
        } else {
            self.until = None;
        }
    }
    fn current(&self, now: Instant) -> Option<u64> {
        self.until
            .filter(|until| now < *until)
            .map(|_| self.generation)
    }
    fn consume(&mut self, generation: u64, now: Instant) -> bool {
        if self.current(now) != Some(generation) {
            return false;
        }
        self.until = None;
        true
    }
    fn failed_after_admission(&mut self, generation: u64) {
        if self.generation == generation && self.until.is_none() {
            self.retry_required = true;
        }
    }
    fn authenticated(&mut self) {
        self.retry_required = false;
    }
}

fn port() -> u16 {
    parse_port(std::env::var("CC_BROWSER_PORT").ok().as_deref())
}

/// Port 0 would bind an ephemeral port the extension can never find.
fn parse_port(value: Option<&str>) -> u16 {
    value
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(PORT_DEFAULT)
}

fn writable_secret_path(name: &str) -> std::path::PathBuf {
    crate::paths::app_runtime_config_dir().join(name)
}

fn bridge() -> &'static Bridge {
    BRIDGE.get_or_init(|| Bridge {
        connection: Mutex::new(None),
        connected: AtomicBool::new(false),
        legacy_update_required: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        next_connection_epoch: AtomicU64::new(0),
        keys: keys::KeyStore::new(writable_secret_path("cc_browser_bootstrap_v8.txt")),
        events: Mutex::new(VecDeque::new()),
        pairing_window: Mutex::new(PairingWindow::default()),
        ext_id: Mutex::new(None),
    })
}

/// Open a 10-minute window during which a fresh-or-stale extension is auto-paired
/// (with our current secret). Generous because installing the unpacked extension
/// can be slow (vision waits, user chatter) - a short window expired before the
/// extension finished loading, leaving a stale secret looping on "bad code".
pub(super) fn open_pairing_window() {
    bridge()
        .pairing_window
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .open(Instant::now() + Duration::from_secs(600));
}

fn in_pairing_window() -> bool {
    pairing_generation().is_some()
}

fn pairing_generation() -> Option<u64> {
    bridge()
        .pairing_window
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .current(Instant::now())
}

/// Whether a setup pairing window is currently open (surfaced in `browser_status`).
pub(super) fn pairing_window_open() -> bool {
    in_pairing_window()
}

pub(super) fn pairing_retry_required() -> bool {
    bridge()
        .pairing_window
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retry_required
}

pub(crate) fn is_connected() -> bool {
    bridge().connected.load(Ordering::SeqCst)
}

pub(super) fn legacy_update_required() -> bool {
    bridge().legacy_update_required.load(Ordering::SeqCst)
}

/// The per-install bootstrap key to stamp into the extension's files so it can
/// mutually authenticate the first connection. Not exposed to the model.
pub(super) fn bootstrap_secret() -> anyhow::Result<String> {
    Ok(bridge().keys.current()?.bootstrap.clone())
}

pub(super) fn pairing_key_error() -> Option<&'static str> {
    bridge().keys.current().err().map(secret::Error::code)
}

pub(super) fn port_for_display() -> u16 {
    port()
}

/// Start the local server once (idempotent). The extension connects in.
pub(crate) fn ensure_started() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(server::listen_loop);
    });
}

fn is_transient(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io)
        if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// Remember the extension's id (trust-on-first-use) for the Origin pin.
fn commit_ext_id(id: Option<String>) {
    if let Some(id) = id {
        *bridge().ext_id.lock().unwrap() = Some(id);
    }
}

fn send(ws: &mut BridgeSocket, v: Value) -> anyhow::Result<()> {
    ws.write(Message::Text(v.to_string().into()))?;
    ws.flush()?;
    Ok(())
}

fn record_event(v: Value) {
    let v = events::bounded_event(v);
    let mut q = bridge().events.lock().unwrap();
    if q.len() >= EVENT_RING {
        q.pop_front();
    }
    q.push_back(v);
}

/// Send one envelope and block for its correlated reply.
pub(super) fn request(json_msg: Value) -> anyhow::Result<Value> {
    let cancel = super::readiness::current_cancel();
    let timeout = super::readiness::bounded_request_timeout(REQ_TIMEOUT);
    if timeout.is_zero() {
        anyhow::bail!("browser request deadline elapsed before dispatch");
    }
    request_with(json_msg, cancel, timeout, None, false)
}

/// Dispatch bounded compensating cleanup after an input edge. Cleanup must not
/// inherit user cancellation: its only purpose is to release already-held input.
pub(super) fn request_cleanup(json_msg: Value) -> anyhow::Result<Value> {
    request_with(json_msg, None, CLEANUP_REQ_TIMEOUT, None, true)
}

pub(super) fn request_on_epoch(json_msg: Value, epoch: u64) -> anyhow::Result<Value> {
    let cancel = super::readiness::current_cancel();
    let timeout = super::readiness::bounded_request_timeout(REQ_TIMEOUT);
    request_with(json_msg, cancel, timeout, Some(epoch), false)
}

pub(super) fn request_cleanup_until(
    json_msg: Value,
    epoch: u64,
    deadline: Instant,
) -> anyhow::Result<Value> {
    let timeout = deadline
        .saturating_duration_since(Instant::now())
        .min(CLEANUP_REQ_TIMEOUT);
    request_with(json_msg, None, timeout, Some(epoch), true)
}

fn request_with(
    json_msg: Value,
    cancel: Option<Arc<AtomicBool>>,
    timeout: Duration,
    expected_epoch: Option<u64>,
    cleanup: bool,
) -> anyhow::Result<Value> {
    super::bridge_wait::ensure_dispatch_allowed(cancel.as_deref())?;
    if !cleanup && !super::effect_barrier::ready() {
        return Err(super::effect_barrier::EffectSettling.into());
    }
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
        .send(PumpInput::Request(Outbound {
            json: json_msg,
            reply: rtx,
            cancel: cancel.clone(),
            deadline,
        }))
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
    match response.get("code").and_then(Value::as_str) {
        Some("ERR_BROWSER_DEBUGGER_REVOKED") => {
            return super::errors::DebuggerAccess::Revoked.into();
        }
        Some("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE") => {
            return super::errors::DebuggerAccess::StorageUnavailable.into();
        }
        _ => {}
    }
    if response.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_TOO_MANY_FRAMES") {
        let detail = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("page has too many frames");
        return super::frame_inventory::TooManyFrames(detail.to_string()).into();
    }
    if response.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_EFFECT_SETTLING") {
        return super::effect_barrier::EffectSettling.into();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_port_override_must_name_a_fixed_port() {
        assert_eq!(parse_port(None), PORT_DEFAULT);
        assert_eq!(parse_port(Some("47801")), 47801);
        assert_eq!(parse_port(Some(" 47801 ")), 47801);
        for invalid in ["0", "", "70000", "-1", "port"] {
            assert_eq!(parse_port(Some(invalid)), PORT_DEFAULT, "{invalid}");
        }
    }
}
