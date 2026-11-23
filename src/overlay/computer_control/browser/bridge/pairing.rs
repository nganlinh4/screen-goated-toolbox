//! Mutual authentication of one extension connection. Key selection and proof
//! checks are pure decisions; `do_pairing` only sequences them over the socket.
use super::super::bridge_auth;
use super::*;

pub(super) struct PairingHooks<'a> {
    pub secret: &'a str,
    pub bootstrap: &'a str,
    pub pairing_generation: &'a dyn Fn() -> Option<u64>,
    pub consume_pairing: &'a dyn Fn(u64) -> bool,
    pub failed_pairing: &'a dyn Fn(u64),
    pub origin_allowed: &'a dyn Fn(&tungstenite::http::HeaderMap) -> bool,
    pub legacy_hint: &'a dyn Fn(),
    pub claim_session: &'a dyn Fn() -> Option<server::SessionGuard>,
    /// Longest wait for each unauthenticated message (`hello`, then `auth`).
    /// A peer that cannot prove a key holds a pre-auth slot only this long.
    pub proof_phase: Duration,
}

pub(super) struct PairingReservation<'a> {
    pub generation: u64,
    pub failure: &'a dyn Fn(u64),
    pub completed: bool,
}
impl Drop for PairingReservation<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (self.failure)(self.generation);
            }));
        }
    }
}

pub(super) struct PairingMetadata {
    pub protocol: u64,
    pub capabilities: Option<Value>,
    pub ext_id: Option<String>,
}

pub(super) struct AuthenticatedPair {
    pub session: bridge_auth::Session,
    pub owner: server::SessionGuard,
    pub metadata: PairingMetadata,
}

/// The fields of the extension's unauthenticated `hello`.
#[derive(Debug, PartialEq)]
struct Hello {
    protocol: u64,
    capabilities: Option<Value>,
    ext_id: Option<String>,
    has_secret: bool,
    has_bootstrap: bool,
    client_nonce: String,
}

impl Hello {
    fn parse(hello: &Value) -> Self {
        Self {
            protocol: hello
                .get("bridgeProtocol")
                .and_then(Value::as_u64)
                .unwrap_or(1),
            capabilities: hello.get("capabilities").cloned(),
            ext_id: hello
                .get("extId")
                .and_then(Value::as_str)
                .map(str::to_string),
            has_secret: hello.get("hasSecret").and_then(Value::as_bool) == Some(true),
            has_bootstrap: hello.get("hasBootstrap").and_then(Value::as_bool) == Some(true),
            client_nonce: hello
                .get("clientNonce")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }
    }
}

/// Which key, if any, the host challenges a `hello` with.
#[derive(Debug, PartialEq)]
enum KeyChoice {
    /// Too old a protocol (`legacy`) or a malformed nonce: the extension must update.
    UpdateRequired { legacy: bool },
    /// Neither a durable key nor an open pairing window applies.
    NotPaired,
    Challenge {
        kind: &'static str,
        key: String,
        bootstrap_generation: Option<u64>,
    },
}

/// The bootstrap key is only usable while a setup pairing window is open; any
/// other client must prove the durable key. A challenge whose key is empty (the
/// install's key could not be loaded) is refused before any proof is sent.
fn choose_key(
    hello: &Hello,
    window: Option<u64>,
    bootstrap: &str,
    durable_secret: &str,
) -> KeyChoice {
    let legacy = hello.protocol < bridge_auth::PROTOCOL;
    if legacy || !bridge_auth::valid_nonce(&hello.client_nonce) {
        return KeyChoice::UpdateRequired { legacy };
    }
    if window.is_some() && hello.has_bootstrap && !bootstrap.is_empty() {
        return KeyChoice::Challenge {
            kind: "bootstrap",
            key: bridge_auth::bootstrap_key(bootstrap),
            bootstrap_generation: window,
        };
    }
    if hello.has_secret {
        return KeyChoice::Challenge {
            kind: "secret",
            key: durable_secret.to_owned(),
            bootstrap_generation: None,
        };
    }
    KeyChoice::NotPaired
}

/// Whether `auth` carries the client-side proof for this exact challenge.
fn client_proof_matches(key: &str, kind: &str, client: &str, server: &str, auth: &Value) -> bool {
    let mac = auth.get("mac").and_then(Value::as_str).unwrap_or("");
    let expected = bridge_auth::proof(key, "client", kind, client, server);
    !key.is_empty() && crypto::ct_eq(mac.as_bytes(), expected.as_bytes())
}

/// Read the next text frame of a given `type`, within the deadline.
fn wait_for_type(ws: &mut BridgeSocket, deadline: Instant, ty: &str) -> anyhow::Result<Value> {
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

pub(super) fn do_pairing(
    ws: &mut BridgeSocket,
    deadline: Instant,
    headers: &tungstenite::http::HeaderMap,
    hooks: &PairingHooks<'_>,
) -> anyhow::Result<Option<AuthenticatedPair>> {
    let phase_deadline = |ws: &mut BridgeSocket| {
        let phase = deadline.min(Instant::now() + hooks.proof_phase);
        ws.get_mut().set_deadline(phase);
        phase
    };
    let hello_deadline = phase_deadline(ws);
    let hello = Hello::parse(&wait_for_type(ws, hello_deadline, "hello")?);
    let (key_kind, key, bootstrap_generation) = match choose_key(
        &hello,
        (hooks.pairing_generation)(),
        hooks.bootstrap,
        hooks.secret,
    ) {
        KeyChoice::UpdateRequired { legacy } => {
            if legacy {
                (hooks.legacy_hint)();
            }
            send(
                ws,
                json!({"type":"error", "error":"browser extension update required"}),
            )?;
            return Ok(None);
        }
        KeyChoice::NotPaired => {
            send(
                ws,
                json!({"type":"error", "error":"not paired - run browser_setup first"}),
            )?;
            return Ok(None);
        }
        KeyChoice::Challenge {
            kind,
            key,
            bootstrap_generation,
        } => (kind, key, bootstrap_generation),
    };
    anyhow::ensure!(!key.is_empty(), "browser pairing key unavailable");
    let client = hello.client_nonce.as_str();
    let server = crypto::random_hex(32)?;
    let mac = bridge_auth::proof(&key, "server", key_kind, client, &server);
    send(
        ws,
        json!({"type":"challenge", "nonce":server, "use":key_kind, "mac":mac}),
    )?;
    let auth_deadline = phase_deadline(ws);
    let auth = wait_for_type(ws, auth_deadline, "auth")?;
    if !client_proof_matches(&key, key_kind, client, &server, &auth) {
        return Ok(None);
    }
    ws.get_mut().set_deadline(deadline);
    let window_current =
        || bootstrap_generation.is_none() || (hooks.pairing_generation)() == bootstrap_generation;
    if !window_current() {
        return Ok(None);
    }
    if !(hooks.origin_allowed)(headers) || !origin::matches_claim(headers, hello.ext_id.as_deref())
    {
        return Ok(None);
    }
    let Some(owner) = (hooks.claim_session)() else {
        return Ok(None);
    };
    if !(hooks.origin_allowed)(headers) || !window_current() {
        return Ok(None);
    }
    if let Some(generation) = bootstrap_generation
        && !(hooks.consume_pairing)(generation)
    {
        return Ok(None);
    }
    let mut reservation = bootstrap_generation.map(|generation| PairingReservation {
        generation,
        failure: hooks.failed_pairing,
        completed: false,
    });
    let mut session = bridge_auth::Session::new(&key, client, &server);
    if key_kind == "bootstrap" {
        send(ws, session.seal(&json!({"type":"pair"}))?)?;
    }
    send(ws, session.seal(&json!({"type":"ready"}))?)?;
    let acknowledgement = session.open(&wait_for_type(ws, deadline, "secure")?)?;
    anyhow::ensure!(
        acknowledgement.get("type").and_then(Value::as_str) == Some("ready_ack"),
        "browser extension did not acknowledge authentication"
    );
    if let Some(reservation) = &mut reservation {
        reservation.completed = true;
    }
    Ok(Some(AuthenticatedPair {
        session,
        owner,
        metadata: PairingMetadata {
            protocol: hello.protocol,
            capabilities: hello.capabilities,
            ext_id: hello.ext_id,
        },
    }))
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
