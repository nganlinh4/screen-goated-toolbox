//! Structural session failure evidence, independent of rendered error text.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;

#[derive(Debug)]
pub(in crate::overlay::computer_control) struct SessionStopped;

impl std::fmt::Display for SessionStopped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stopped")
    }
}

impl std::error::Error for SessionStopped {}

#[derive(Debug)]
pub(in crate::overlay::computer_control) struct ProviderSetupFailure {
    pub(in crate::overlay::computer_control) message: String,
    pub(in crate::overlay::computer_control) resource_limited: bool,
}

impl std::fmt::Display for ProviderSetupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Live setup rejected: {}", self.message)
    }
}

impl std::error::Error for ProviderSetupFailure {}

pub(in crate::overlay::computer_control) fn check_connection_attempt(
    stop: &AtomicBool,
    attempt_stop: Option<&AtomicBool>,
    deadline: Option<Instant>,
) -> Result<()> {
    if stop.load(Ordering::SeqCst)
        || attempt_stop.is_some_and(|attempt| attempt.load(Ordering::SeqCst))
    {
        return Err(SessionStopped.into());
    }
    anyhow::ensure!(
        deadline.is_none_or(|deadline| Instant::now() < deadline),
        "connection attempt deadline elapsed"
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::overlay::computer_control) enum SessionFailureKind {
    Stopped,
    Credential,
    ResourceLimit,
    Other,
}

pub(in crate::overlay::computer_control) fn failure_kind(
    error: &anyhow::Error,
) -> SessionFailureKind {
    if error.is::<SessionStopped>() {
        SessionFailureKind::Stopped
    } else if super::api_key_error_code(error).is_some() {
        SessionFailureKind::Credential
    } else if error.chain().any(|cause| {
        cause
            .downcast_ref::<ProviderSetupFailure>()
            .is_some_and(|failure| failure.resource_limited)
            || matches!(cause.downcast_ref::<tungstenite::Error>(),
                Some(tungstenite::Error::Http(response)) if response.status().as_u16() == 429)
    }) {
        SessionFailureKind::ResourceLimit
    } else {
        SessionFailureKind::Other
    }
}

#[cfg(test)]
mod tests;
