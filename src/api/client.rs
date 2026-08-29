use crate::APP;
use std::sync::LazyLock;
use std::time::Duration;
use ureq::http::HeaderMap;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

const STREAM_RESPONSE_START_TIMEOUT: Duration = Duration::from_secs(120);
const STREAM_PROGRESS_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

fn platform_tls_config() -> TlsConfig {
    TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build()
}

/// Build a ureq agent carrying our user-agent string and an end-to-end timeout.
fn build_agent(timeout_global: Duration, http_status_as_error: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_global(Some(timeout_global))
        .http_status_as_error(http_status_as_error)
        .tls_config(platform_tls_config())
        .build()
        .into()
}

fn build_download_agent() -> ureq::Agent {
    build_download_agent_with_read_timeout(Duration::from_secs(30))
}

fn build_download_agent_with_read_timeout(read_timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(120)))
        .timeout_recv_body(Some(read_timeout))
        .tls_config(platform_tls_config())
        .build()
        .into()
}

fn build_stream_agent(http_status_as_error: bool) -> ureq::Agent {
    build_stream_agent_with_timeouts(
        http_status_as_error,
        STREAM_RESPONSE_START_TIMEOUT,
        STREAM_PROGRESS_IDLE_TIMEOUT,
    )
}

fn build_stream_agent_with_timeouts(
    http_status_as_error: bool,
    response_start_timeout: Duration,
    progress_idle_timeout: Duration,
) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(response_start_timeout))
        .timeout_recv_body(Some(progress_idle_timeout))
        .http_status_as_error(http_status_as_error)
        .tls_config(platform_tls_config())
        .build()
        .into()
}

/// Apply a tighter end-to-end budget to one request without changing the
/// shared agent's defaults for unrelated long-running work.
pub fn with_request_timeout<B>(
    request: ureq::RequestBuilder<B>,
    timeout: Option<Duration>,
) -> ureq::RequestBuilder<B> {
    match timeout {
        Some(timeout) => request.config().timeout_global(Some(timeout)).build(),
        None => request,
    }
}

/// Agent for unary (non-streaming) requests — bounded end-to-end at 120s.
pub static UREQ_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(120), true));

/// Unary agent that returns HTTP error responses so callers can inspect provider
/// retry headers and structured error bodies before deciding how to recover.
pub static UREQ_RESPONSE_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(120), false));

/// Agent for streaming (SSE) requests. Response start and each body-read stall are
/// bounded independently; there is no whole-response deadline while bytes keep
/// arriving.
pub static UREQ_STREAM_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| build_stream_agent(true));

/// Streaming agent that preserves HTTP error responses for bounded provider
/// diagnostics while retaining the long-lived SSE timeout.
pub static UREQ_STREAM_RESPONSE_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_stream_agent(false));

/// Download agent with bounded connection/header phases and a 30-second idle
/// body-read timeout, but no whole-body deadline. Large verified files may take
/// hours while a stalled read still returns control to cancellation-aware callers.
pub static UREQ_DOWNLOAD_AGENT: LazyLock<ureq::Agent> = LazyLock::new(build_download_agent);

/// True when a ureq error is an HTTP 401/403 (authentication failure).
///
/// In ureq 3.x an error status surfaces as the typed `Error::StatusCode`, so this
/// matches the code directly instead of substring-scanning the Display text — which
/// false-positives on a transport error whose URL or body happens to contain
/// "401"/"403" and can wrongly flag a provider's key as invalid (and permanently
/// block it). Also covers 403, which several call sites previously missed.
pub fn is_auth_error(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::StatusCode(401 | 403))
}

#[cfg(test)]
mod download_agent_tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    #[test]
    fn body_read_timeout_is_idle_bounded() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });

        let agent = super::build_download_agent_with_read_timeout(Duration::from_millis(40));
        let response = agent.get(format!("http://{address}/model")).call().unwrap();
        let mut reader = response.into_body().into_reader();
        let started = Instant::now();
        let error = reader.read(&mut [0_u8; 1]).unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(error.to_string().to_ascii_lowercase().contains("timeout"));
        server.join().unwrap();
    }

    #[test]
    fn streaming_progress_can_outlive_one_idle_window() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            stream.flush().unwrap();
            for _ in 0..5 {
                std::thread::sleep(Duration::from_millis(150));
                stream.write_all(b"1\r\nx\r\n").unwrap();
                stream.flush().unwrap();
            }
            stream.write_all(b"0\r\n\r\n").unwrap();
        });

        let agent = super::build_stream_agent_with_timeouts(
            true,
            // The response-start budget is not the invariant under test. Keep it
            // comfortably above a saturated parallel test runner's scheduling
            // delay while retaining the deliberately short body-idle window.
            Duration::from_secs(2),
            Duration::from_millis(500),
        );
        let started = Instant::now();
        let response = agent
            .get(format!("http://{address}/stream"))
            .call()
            .unwrap();
        let mut body = String::new();
        response
            .into_body()
            .into_reader()
            .read_to_string(&mut body)
            .unwrap();

        assert_eq!(body, "xxxxx");
        assert!(started.elapsed() > Duration::from_millis(600));
        server.join().unwrap();
    }
}

/// Capture the provider's latest typed rate-limit snapshot for one API endpoint.
///
/// Call this before converting an exposed HTTP error response into an error so
/// useful 429 headers are retained. Providers with shared quota scopes are
/// normalized by `usage_key_for_response`.
pub fn record_usage_headers(provider: &str, full_name: &str, headers: &HeaderMap) {
    let Some(snapshot) = crate::usage_stats::snapshot_from_headers(
        provider,
        headers,
        crate::usage_stats::now_unix_seconds(),
    ) else {
        return;
    };
    let key = crate::usage_stats::usage_key_for_response(provider, full_name);
    #[cfg(not(feature = "recorder-worker"))]
    crate::retry_model_chain::record_token_budget(provider, full_name, headers);
    if let Ok(mut app) = APP.lock() {
        app.model_usage_stats.insert(key, snapshot);
    }
}

pub fn record_usage_simple(headers: &HeaderMap, stats_key: &str) {
    record_usage_headers("groq", stats_key, headers);
}

/// Log Groq's automatic prompt-cache contribution without changing quota UI.
/// Cache hits are response metadata; no request flag enables them.
pub fn record_groq_json_usage(stats_key: &str, root: &serde_json::Value) {
    let Some(usage) = root.get("usage") else {
        return;
    };
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cached_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if prompt_tokens > 0 {
        crate::log_info!(
            "[Groq][cache] model={} cached_tokens={}/{} ({:.1}%)",
            stats_key,
            cached_tokens,
            prompt_tokens,
            cached_tokens as f64 * 100.0 / prompt_tokens as f64
        );
    }
}

#[cfg(test)]
mod tls_tests {
    use super::*;
    use std::net::TcpListener;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn assert_platform_tls(agent: &ureq::Agent) {
        let tls = agent.config().tls_config();
        assert_eq!(tls.provider(), TlsProvider::NativeTls);
        assert!(matches!(tls.root_certs(), RootCerts::PlatformVerifier));
    }

    #[test]
    fn every_shared_agent_uses_the_windows_certificate_verifier() {
        for agent in [
            &*UREQ_AGENT,
            &*UREQ_RESPONSE_AGENT,
            &*UREQ_STREAM_AGENT,
            &*UREQ_STREAM_RESPONSE_AGENT,
            &*UREQ_DOWNLOAD_AGENT,
        ] {
            assert_platform_tls(agent);
        }
    }

    #[test]
    fn native_tls_connector_is_present_for_https_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            build_agent(Duration::from_secs(2), true)
                .get(format!("https://{address}/"))
                .call()
        }));
        server.join().unwrap();
        assert!(
            result.is_ok(),
            "configured NativeTls provider was not compiled"
        );
        assert!(
            result.unwrap().is_err(),
            "loopback server unexpectedly spoke TLS"
        );
    }
}
