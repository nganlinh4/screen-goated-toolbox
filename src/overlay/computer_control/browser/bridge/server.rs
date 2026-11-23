//! Bounded unauthenticated admission and exclusive authenticated ownership.
use super::*;
use std::sync::LazyLock;
use std::sync::atomic::AtomicUsize;
use tungstenite::protocol::WebSocketConfig;

const MAX_PREAUTH: usize = 4;
const PREAUTH_TIMEOUT: Duration = Duration::from_secs(12);
const PREAUTH_BYTES: usize = 64 * 1024;
/// Upgrade, `hello` and `auth` each must arrive within this; the extension sends
/// them immediately, so only an unauthenticated idler ever waits this long.
const PROOF_PHASE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Admission {
    preauth: AtomicUsize,
    active: Mutex<bool>,
}
static ADMISSION: LazyLock<Arc<Admission>> = LazyLock::new(|| Arc::new(Admission::default()));

struct Preauth(Arc<Admission>);
impl Drop for Preauth {
    fn drop(&mut self) {
        self.0.preauth.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Admission {
    fn preauth(self: &Arc<Self>) -> Option<Preauth> {
        self.preauth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_PREAUTH).then_some(n + 1)
            })
            .ok()?;
        Some(Preauth(self.clone()))
    }
    fn claim(self: &Arc<Self>, cleanup: impl FnOnce() + Send + 'static) -> Option<SessionGuard> {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if *active {
            return None;
        }
        *active = true;
        Some(SessionGuard {
            admission: self.clone(),
            cleanup: Some(Box::new(cleanup)),
        })
    }
    fn if_idle(&self, action: impl FnOnce()) {
        let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if !*active {
            action();
        }
    }
}

pub(super) struct SessionGuard {
    admission: Arc<Admission>,
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}
impl Drop for SessionGuard {
    fn drop(&mut self) {
        let mut active = self
            .admission
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cleanup) = self.cleanup.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(cleanup));
        }
        *active = false;
    }
}

pub(super) fn claim_session() -> Option<SessionGuard> {
    let owner = ADMISSION.claim(|| {
        bridge().connected.store(false, Ordering::SeqCst);
        let _ = std::panic::catch_unwind(super::super::capabilities::reset);
        *bridge()
            .connection
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    })?;
    bridge()
        .legacy_update_required
        .store(false, Ordering::SeqCst);
    Some(owner)
}

pub(super) fn legacy_hint() {
    ADMISSION.if_idle(|| {
        bridge()
            .legacy_update_required
            .store(true, Ordering::SeqCst)
    });
}

pub(super) fn listen_loop() {
    let addr = format!("127.0.0.1:{}", port());
    let listener = super::super::bridge_listener::bind_with_retry(&addr);
    telemetry::human("cc-browser", format!("bridge listening on {addr}"));
    telemetry::event(
        "browser_bridge_listening",
        "browser_bridge",
        Privacy::Safe,
        json!({"addr": addr}),
    );
    for stream in listener.incoming().flatten() {
        let Some(permit) = ADMISSION.preauth() else {
            continue;
        };
        let deadline = Instant::now() + PREAUTH_TIMEOUT;
        let _ = std::thread::Builder::new()
            .name("sgt-browser-pair".into())
            .spawn(move || {
                if let Err(error) = handle_conn(stream, deadline, permit) {
                    record_connection_end(&error.to_string());
                }
            });
    }
}

/// Log how a connection ended, throttling identical repeats (a rejected
/// extension retries every few seconds for as long as the app runs).
fn record_connection_end(error: &str) {
    static ENDINGS: LazyLock<Mutex<log_throttle::RepeatThrottle>> = LazyLock::new(Mutex::default);
    let admitted = ENDINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .admit(error, Instant::now());
    let Some(suppressed) = admitted else {
        return;
    };
    telemetry::event(
        "browser_bridge_connection_ended",
        "browser_bridge",
        Privacy::Safe,
        json!({"error": error, "suppressed_repeats": suppressed}),
    );
    telemetry::human("cc-browser", format!("connection ended: {error}"));
}

/// The Origin gate, evaluated against the current pin and pairing window both at
/// the HTTP upgrade and again around session admission.
fn origin_allowed_now(headers: &tungstenite::http::HeaderMap) -> bool {
    let pinned = bridge()
        .ext_id
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    origin::allowed(headers, pinned.as_deref(), in_pairing_window())
}

#[expect(
    clippy::result_large_err,
    reason = "tungstenite fixes the handshake callback's HTTP error response type"
)]
fn handle_conn(stream: TcpStream, deadline: Instant, permit: Preauth) -> anyhow::Result<()> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(PREAUTH_BYTES))
        .max_frame_size(Some(PREAUTH_BYTES));
    let upgrade_deadline = deadline.min(Instant::now() + PROOF_PHASE);
    let stream = transport::DeadlineStream::new(stream, upgrade_deadline);
    let mut request_headers = None;
    let mut ws = tungstenite::accept_hdr_with_config(
        stream,
        |req: &Request, resp: Response| {
            if origin_allowed_now(req.headers()) {
                request_headers = Some(req.headers().clone());
                Ok(resp)
            } else {
                Err(tungstenite::http::Response::builder()
                    .status(tungstenite::http::StatusCode::FORBIDDEN)
                    .body(Some("unexpected browser origin".into()))
                    .unwrap())
            }
        },
        Some(config),
    )
    .map_err(|error| anyhow::anyhow!("ws handshake: {error}"))?;
    let headers = request_headers.ok_or_else(|| anyhow::anyhow!("missing upgrade request"))?;
    // A key that cannot be loaded leaves both empty; pairing then fails closed.
    let keys = bridge().keys.current().ok();
    let (secret, bootstrap) = keys.as_deref().map_or(("", ""), |keys| {
        (keys.durable.as_str(), keys.bootstrap.as_str())
    });
    let hooks = PairingHooks {
        secret,
        bootstrap,
        pairing_generation: &pairing_generation,
        consume_pairing: &|generation| {
            bridge()
                .pairing_window
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .consume(generation, Instant::now())
        },
        failed_pairing: &|generation| {
            bridge()
                .pairing_window
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .failed_after_admission(generation)
        },
        origin_allowed: &origin_allowed_now,
        legacy_hint: &legacy_hint,
        claim_session: &claim_session,
        proof_phase: PROOF_PHASE,
    };
    let Some(AuthenticatedPair {
        session,
        owner: _owner,
        metadata,
    }) = do_pairing(&mut ws, deadline, &headers, &hooks)?
    else {
        let _ = ws.close(None);
        anyhow::bail!("pairing failed or an authenticated session is already active");
    };
    // A successful durable reconnect also resolves a prior uncertain pairing
    // attempt; it must not keep telling the user to start setup again.
    bridge()
        .pairing_window
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .authenticated();
    bridge()
        .legacy_update_required
        .store(false, Ordering::SeqCst);
    commit_ext_id(metadata.ext_id);
    super::super::capabilities::negotiate(metadata.protocol, metadata.capabilities.as_ref());
    drop(permit);
    let (tx, rx) = mpsc::channel::<PumpInput>();
    ws.get_mut().authenticated(tx.clone())?;
    ws.set_config(|config| *config = WebSocketConfig::default());
    if super::super::capabilities::supports(super::super::capabilities::EFFECT_ADMISSION) {
        super::super::effect_barrier::start_sync();
    } else {
        super::super::effect_barrier::reset_without_ledger();
    }
    let epoch = bridge()
        .next_connection_epoch
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    *bridge()
        .connection
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(ConnectionHandle { epoch, sender: tx });
    telemetry::human("cc-browser", "extension paired + connected");
    telemetry::event(
        "browser_extension_connected",
        "browser_bridge",
        Privacy::Safe,
        json!({
            "port": port(), "protocol_version": super::super::capabilities::protocol_version(),
            "capabilities": super::super::capabilities::list(), "connection_epoch": epoch,
        }),
    );
    bridge().connected.store(true, Ordering::SeqCst);
    super::super::record_connection();
    let result = pump::run(&mut ws, rx, session, epoch);
    if bridge().connected.swap(false, Ordering::SeqCst) {
        super::super::record_connection();
    }
    result
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
