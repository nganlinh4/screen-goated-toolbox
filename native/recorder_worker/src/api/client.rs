use std::sync::LazyLock;
use std::time::Duration;
use ureq::http::HeaderMap;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

fn platform_tls_config() -> TlsConfig {
    TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build()
}

fn build_agent(http_status_as_error: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(http_status_as_error)
        .tls_config(platform_tls_config())
        .build()
        .into()
}

fn build_download_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(120)))
        .tls_config(platform_tls_config())
        .build()
        .into()
}

pub(crate) static UREQ_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| build_agent(true));
pub(crate) static UREQ_RESPONSE_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| build_agent(false));
pub(crate) static UREQ_DOWNLOAD_AGENT: LazyLock<ureq::Agent> = LazyLock::new(build_download_agent);

pub(crate) fn record_usage_headers(provider: &str, model: &str, headers: &HeaderMap) {
    let Some(snapshot) = crate::usage_stats::snapshot_from_headers(
        provider,
        headers,
        crate::usage_stats::now_unix_seconds(),
    ) else {
        return;
    };
    let key = crate::usage_stats::usage_key_for_response(provider, model);
    if let Ok(mut app) = crate::APP.lock() {
        app.model_usage_stats.insert(key, snapshot);
    }
}

pub(crate) fn record_usage_simple(headers: &HeaderMap, model: &str) {
    record_usage_headers("groq", model, headers);
}

pub(crate) fn record_groq_json_usage(model: &str, root: &serde_json::Value) {
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
            model,
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

    #[test]
    fn every_worker_agent_uses_the_windows_certificate_verifier() {
        for agent in [&*UREQ_AGENT, &*UREQ_RESPONSE_AGENT, &*UREQ_DOWNLOAD_AGENT] {
            let tls = agent.config().tls_config();
            assert_eq!(tls.provider(), TlsProvider::NativeTls);
            assert!(matches!(tls.root_certs(), RootCerts::PlatformVerifier));
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
            build_agent(true).get(format!("https://{address}/")).call()
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
