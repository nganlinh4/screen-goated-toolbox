//! The host's Groq chat sender: every Groq text and vision request goes
//! through it, over the one retry contract the recorder worker also uses.

use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

use anyhow::Result;

use super::chat_loop::ChatResponse;
use crate::api::client::{RequestTimeouts, UREQ_RESPONSE_AGENT};

pub(crate) struct ChatSend<'a> {
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub request_timeout: Option<RequestTimeouts>,
    pub cancel_token: &'a Option<Arc<AtomicBool>>,
    /// Wait out one short provider-declared 429 delay in place instead of
    /// handing the failure to the caller's retry chain.
    pub retry_short_rate_limit: bool,
    /// Prefix for transport failures, naming the calling feature.
    pub transport_label: &'a str,
}

/// Send `payload` with the shared Groq retry contract
/// ([`super::chat_loop::send_with_retries`]) over the host's response agent.
pub(crate) fn send_chat(
    send: ChatSend<'_>,
    payload: &mut serde_json::Value,
    on_retry: impl FnMut(Duration),
) -> Result<ChatResponse> {
    super::chat_loop::send_with_retries(
        payload,
        send.cancel_token,
        send.retry_short_rate_limit,
        |payload| {
            let request = UREQ_RESPONSE_AGENT
                .post(send.endpoint)
                .header("Authorization", &format!("Bearer {}", send.api_key))
                .header("Content-Type", "application/json");
            crate::api::client::with_request_timeouts(request, send.request_timeout)
                .send_json(payload)
                .map_err(|error| {
                    anyhow::anyhow!(crate::api::client::transport_error_message(
                        send.transport_label,
                        &error,
                    ))
                })
        },
        on_retry,
    )
}

#[cfg(test)]
#[path = "chat_send_tests.rs"]
mod tests;
