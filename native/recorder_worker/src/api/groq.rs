//! The worker shares the host's Groq response-format selection and chat retry
//! contract verbatim; it sends over its own agent.

#[path = "../../../../src/api/groq/chat_loop.rs"]
mod chat_loop;
#[path = "../../../../src/api/groq/http_error.rs"]
mod http_error;
#[path = "../../../../src/api/groq/output_limit.rs"]
mod output_limit;
#[path = "../../../../src/api/groq/response_format.rs"]
mod response_format;
#[path = "../../../../src/api/groq/retry.rs"]
mod retry;
pub(crate) use chat_loop::{CHAT_COMPLETIONS_URL, send_with_retries};
use http_error::http_error;
pub(crate) use response_format::structured_response_format;
use retry::{groq_rate_limit_retry_delay, retry_after_seconds, wait_for_groq_retry};
