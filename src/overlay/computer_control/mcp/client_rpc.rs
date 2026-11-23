//! Correlated RPC lifetime, cancellation and conservative dispatch evidence.
use super::super::prose::bounded_prose;
use super::McpClient;
use super::client_writer::{self, FailureKind};
use anyhow::Result;
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
const CANCEL_WRITE_BUDGET: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RequestFailureKind {
    Cancelled,
    Timeout,
    Disconnected,
    InvalidRequest,
    Peer,
}

#[derive(Debug)]
pub(super) struct RequestFailure {
    pub(super) kind: RequestFailureKind,
    pub(super) may_have_been_sent: bool,
    message: String,
}
impl RequestFailure {
    pub(super) fn new(
        kind: RequestFailureKind,
        may_have_been_sent: bool,
        message: impl ToString,
    ) -> Self {
        Self {
            kind,
            may_have_been_sent,
            message: bounded_prose(&message.to_string()),
        }
    }
    pub(super) fn cancelled(may_have_been_sent: bool) -> Self {
        Self::new(
            RequestFailureKind::Cancelled,
            may_have_been_sent,
            "MCP request cancelled",
        )
    }
    fn write(error: client_writer::Failure) -> Self {
        let kind = match error.kind {
            FailureKind::Cancelled => RequestFailureKind::Cancelled,
            FailureKind::Timeout => RequestFailureKind::Timeout,
            FailureKind::Transport => RequestFailureKind::Disconnected,
        };
        Self::new(kind, error.started, error.message)
    }
    fn encoding(error: client_writer::EncodingFailure) -> Self {
        match error {
            client_writer::EncodingFailure::Cancelled => Self::cancelled(false),
            client_writer::EncodingFailure::Timeout => Self::new(
                RequestFailureKind::Timeout,
                false,
                "MCP preparation deadline elapsed",
            ),
            client_writer::EncodingFailure::Invalid(error) => {
                Self::new(RequestFailureKind::InvalidRequest, false, error)
            }
        }
    }
}
impl fmt::Display for RequestFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for RequestFailure {}

struct Pending<'a> {
    client: &'a McpClient,
    id: u64,
    armed: bool,
}

fn reserve_request_id(counter: &AtomicU64) -> std::result::Result<u64, RequestFailure> {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
        .map_err(|_| {
            RequestFailure::new(
                RequestFailureKind::InvalidRequest,
                false,
                "MCP request id exhausted",
            )
        })
}
impl Pending<'_> {
    fn retire(&mut self) -> bool {
        self.armed = false;
        self.client.pending.lock().remove(&self.id).is_some()
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.retire();
            // An unwinding caller must not leave an unobserved mutating RPC.
            self.client.shutdown();
        }
    }
}

impl McpClient {
    #[cfg(test)]
    pub(super) fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.request_until(method, params, timeout, None)
    }

    pub(super) fn request_until(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        stop: Option<&AtomicBool>,
    ) -> Result<Value> {
        self.request_inner(method, params, timeout, stop, None)
    }

    pub(super) fn request_owned(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        stop: &Arc<AtomicBool>,
    ) -> Result<Value> {
        self.request_inner(method, params, timeout, Some(stop), Some(stop.clone()))
    }

    fn request_inner(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        stop: Option<&AtomicBool>,
        owner: Option<Arc<AtomicBool>>,
    ) -> Result<Value> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            RequestFailure::new(
                RequestFailureKind::InvalidRequest,
                false,
                "MCP request deadline is invalid",
            )
        })?;
        self.check_request(deadline, stop)?;
        let id = reserve_request_id(&self.next_id)?;
        let line = client_writer::encode(
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            deadline,
            stop,
        )
        .map_err(RequestFailure::encoding)?;
        self.check_request(deadline, stop)?;
        let (tx, rx) = mpsc::channel();
        self.pending.lock().insert(id, tx);
        let mut pending = Pending {
            client: self,
            id,
            armed: true,
        };
        if let Err(error) = self.writer.send(line, deadline, stop, owner) {
            pending.retire();
            return Err(RequestFailure::write(error).into());
        }
        loop {
            if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
                self.interrupt_request(method, &mut pending);
                return Err(RequestFailure::cancelled(true).into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.interrupt_request(method, &mut pending);
                return Err(RequestFailure::new(
                    RequestFailureKind::Timeout,
                    true,
                    format!("MCP '{method}' timed out"),
                )
                .into());
            }
            match rx.recv_timeout(remaining.min(POLL)) {
                Ok(response) => {
                    pending.retire();
                    if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
                        return Err(RequestFailure::cancelled(true).into());
                    }
                    if Instant::now() >= deadline {
                        return Err(RequestFailure::new(
                            RequestFailureKind::Timeout,
                            true,
                            format!("MCP '{method}' timed out"),
                        )
                        .into());
                    }
                    if let Some(error) = response.get("error") {
                        return Err(RequestFailure::new(
                            RequestFailureKind::Peer,
                            true,
                            error
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("MCP peer rejected the request"),
                        )
                        .into());
                    }
                    return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    pending.retire();
                    return Err(RequestFailure::new(
                        RequestFailureKind::Disconnected,
                        true,
                        format!("MCP server disconnected during '{method}'"),
                    )
                    .into());
                }
            }
        }
    }

    fn check_request(&self, deadline: Instant, stop: Option<&AtomicBool>) -> Result<()> {
        if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
            return Err(RequestFailure::cancelled(false).into());
        }
        if Instant::now() >= deadline {
            return Err(RequestFailure::new(
                RequestFailureKind::Timeout,
                false,
                "MCP request deadline elapsed before dispatch",
            )
            .into());
        }
        if !self.alive.load(Ordering::Acquire) {
            return Err(RequestFailure::new(
                RequestFailureKind::Disconnected,
                false,
                "MCP server is not running",
            )
            .into());
        }
        Ok(())
    }

    fn interrupt_request(&self, method: &str, pending: &mut Pending<'_>) {
        if !pending.retire() {
            return;
        } // The reply already settled or transport closed.
        if method == "initialize" {
            self.shutdown(); // MCP forbids cancelling initialize by notification.
            return;
        }
        let deadline = Instant::now() + CANCEL_WRITE_BUDGET;
        let message = json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":pending.id}});
        let sent = client_writer::encode(&message, deadline, None)
            .ok()
            .is_some_and(|line| self.writer.send(line, deadline, None, None).is_ok());
        if !sent {
            self.shutdown();
        }
    }

    pub(super) fn notify_until(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        stop: Option<&AtomicBool>,
    ) -> Result<()> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            RequestFailure::new(
                RequestFailureKind::InvalidRequest,
                false,
                "MCP notification deadline is invalid",
            )
        })?;
        self.check_request(deadline, stop)?;
        let line = client_writer::encode(
            &json!({"jsonrpc":"2.0","method":method,"params":params}),
            deadline,
            stop,
        )
        .map_err(RequestFailure::encoding)?;
        self.writer
            .send(line, deadline, stop, None)
            .map_err(|error| RequestFailure::write(error).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_request_identity_cannot_wrap_or_reuse_a_reply_slot() {
        let counter = AtomicU64::new(1);
        assert_eq!(reserve_request_id(&counter).unwrap(), 1);
        assert_eq!(reserve_request_id(&counter).unwrap(), 2);
        counter.store(u64::MAX, Ordering::SeqCst);
        let error = reserve_request_id(&counter).unwrap_err();
        assert_eq!(error.kind, RequestFailureKind::InvalidRequest);
        assert!(!error.may_have_been_sent);
        assert_eq!(counter.load(Ordering::SeqCst), u64::MAX);
    }
}
