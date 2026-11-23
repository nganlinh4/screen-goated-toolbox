//! Shared Groq capabilities. Provider behavior lives here so callers do not
//! grow model-name conditionals or implement incompatible retry policies.

mod chat_loop;
mod chat_send;
pub(crate) mod compound;
mod http_error;
mod response_format;
mod retry;
pub(crate) use chat_loop::CHAT_COMPLETIONS_URL;
pub(crate) use chat_send::{ChatSend, send_chat};
pub(crate) use http_error::http_error;
pub(crate) use response_format::structured_response_format;
pub(crate) use retry::{
    groq_rate_limit_retry_delay, retry_after_seconds, wait_for_groq_retry, wait_unless_cancelled,
};

pub(crate) mod output_limit;
