//! The authenticated request/reply pump. It sleeps on one channel that carries
//! both outbound requests and inbound socket bytes, so either wakes it at once.
use super::super::{bridge_auth, bridge_wait, effect_barrier};
use super::admission::{grant_if_owned, send_grant, valid_effect_token};
use super::*;
use std::collections::HashMap;
use std::sync::mpsc::RecvTimeoutError;

#[derive(PartialEq)]
enum Flow {
    Open,
    Closed,
}

struct Pump<'a> {
    ws: &'a mut BridgeSocket,
    session: bridge_auth::Session,
    pending: HashMap<u64, bridge_wait::PendingReply>,
    epoch: u64,
}

pub(super) fn run(
    ws: &mut BridgeSocket,
    rx: mpsc::Receiver<PumpInput>,
    session: bridge_auth::Session,
    epoch: u64,
) -> anyhow::Result<()> {
    let mut pump = Pump {
        ws,
        session,
        pending: HashMap::new(),
        epoch,
    };
    // Frames buffered by the WebSocket during pairing are readable immediately.
    if pump.read_available()? == Flow::Closed {
        return Ok(());
    }
    loop {
        pump.cancel_inactive()?;
        match rx.recv_timeout(PENDING_SWEEP_TICK) {
            Ok(PumpInput::Request(out)) => pump.dispatch(out)?,
            Ok(PumpInput::Inbound(inbound)) => {
                pump.ws.get_mut().feed(inbound);
                if pump.read_available()? == Flow::Closed {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

impl Pump<'_> {
    fn seal_and_send(&mut self, body: &Value) -> anyhow::Result<()> {
        let frame = self.session.seal(body)?;
        send(self.ws, frame)
    }

    fn cancel_inactive(&mut self) -> anyhow::Result<()> {
        for id in bridge_wait::prune_inactive(&mut self.pending, Instant::now()) {
            self.seal_and_send(&json!({"type":"request_cancel", "requestId":id}))?;
        }
        Ok(())
    }

    fn dispatch(&mut self, mut out: Outbound) -> anyhow::Result<()> {
        if Instant::now() >= out.deadline
            || out
                .cancel
                .as_deref()
                .is_some_and(|token| token.load(Ordering::SeqCst))
        {
            return Ok(());
        }
        let effect = out.json.get("type").and_then(Value::as_str) != Some("frames");
        let token = if effect {
            let remaining = out
                .deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(15_000) as u64;
            if remaining == 0 {
                return Ok(());
            }
            out.json["timeoutMs"] = json!(remaining);
            let token = if out.json.get("type").and_then(Value::as_str) == Some("runtime")
                && out.json.get("action").and_then(Value::as_str) == Some("reload")
            {
                valid_effect_token(out.json.get("effectToken"))
                    .map(str::to_owned)
                    .unwrap_or(crypto::random_hex(16)?)
            } else {
                crypto::random_hex(16)?
            };
            out.json["effectToken"] = json!(token);
            Some(token)
        } else {
            None
        };
        if let Some(id) = out.json.get("id").and_then(Value::as_u64) {
            self.pending.insert(
                id,
                bridge_wait::PendingReply::new(out.reply, out.cancel, out.deadline, token),
            );
        }
        self.seal_and_send(&out.json)
    }

    /// Handle every complete frame currently buffered, without blocking.
    fn read_available(&mut self) -> anyhow::Result<Flow> {
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    let frame: Value = serde_json::from_str(&t)?;
                    let v = self.session.open(&frame)?;
                    self.handle(v)?;
                }
                Ok(Message::Ping(p)) => {
                    let _ = self.ws.send(Message::Pong(p));
                }
                Ok(Message::Close(_)) => return Ok(Flow::Closed),
                Ok(_) => {}
                Err(e) if is_transient(&e) => return Ok(Flow::Open),
                Err(e) => anyhow::bail!("read: {e}"),
            }
        }
    }

    fn handle(&mut self, v: Value) -> anyhow::Result<()> {
        match v.get("type").and_then(Value::as_str) {
            Some("cdp_dispatch_ready") => {
                if let Some(id) = v.get("requestId").and_then(Value::as_u64) {
                    self.grant(id)?;
                }
            }
            Some("effect_terminal") => {
                if let Some(token) = valid_effect_token(v.get("effectToken")) {
                    self.terminal(token)?;
                }
            }
            Some("effect_ledger_snapshot") => sync_ledger(&v)?,
            _ => {
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    if let Some(reply) = self.pending.remove(&id) {
                        if reply.was_admitted()
                            && let Some(token) = reply.token()
                        {
                            anyhow::ensure!(
                                effect_barrier::settle(token),
                                "too many browser terminal receipts during synchronization"
                            );
                        }
                        reply.deliver(v);
                    }
                } else if v.get("type").and_then(Value::as_str) == Some("event") {
                    record_event(v);
                }
            }
        }
        Ok(())
    }

    fn grant(&mut self, id: u64) -> anyhow::Result<()> {
        let Self {
            ws,
            session,
            pending,
            ..
        } = self;
        let granted = if let Some(request) = pending.get_mut(&id) {
            grant_if_owned(request, effect_barrier::begin, |remaining| {
                send_grant(ws, session, id, remaining)
            })?
        } else {
            false
        };
        if !granted {
            send_grant(ws, session, id, 0)?;
        }
        Ok(())
    }

    fn terminal(&mut self, token: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            effect_barrier::settle(token),
            "too many browser terminal receipts during synchronization"
        );
        super::super::reload_receipt::record(token, self.epoch);
        for request in self.pending.values_mut() {
            if request.was_admitted() && request.token() == Some(token) {
                request.mark_terminal();
            }
        }
        self.seal_and_send(&json!({"type":"effect_terminal_ack", "effectToken":token}))
    }
}

fn sync_ledger(v: &Value) -> anyhow::Result<()> {
    let Some(values) = v.get("active").and_then(Value::as_array) else {
        anyhow::bail!("invalid browser effect ledger");
    };
    anyhow::ensure!(
        values.len() <= 256,
        "browser effect ledger exceeds its limit"
    );
    let tokens = values
        .iter()
        .map(|value| {
            valid_effect_token(Some(value))
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("invalid browser effect token"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        effect_barrier::sync(&tokens),
        "browser effect ledger is full"
    );
    Ok(())
}

#[cfg(test)]
#[path = "pump_tests.rs"]
mod tests;
