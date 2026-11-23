//! Taalas / chatjimmy.ai API client shared by translation, refinement and detection.

#[cfg(not(feature = "recorder-worker"))]
use crate::api::client::RequestTimeouts;
use crate::api::client::UREQ_RESPONSE_AGENT;
/// The worker's client has no phased timeouts; one deadline bounds the call.
#[cfg(feature = "recorder-worker")]
type RequestTimeouts = std::time::Duration;
use anyhow::{Result, bail};
use std::io::Read;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const ENDPOINT: &str = "https://chatjimmy.ai/api/chat";
const MODEL: &str = "llama3.1-8B";
const TOP_K: u8 = 8;
const STATS_MARKER: &str = "<|stats|>";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ERROR_BYTES: u64 = 8 * 1024;

/// Request and decode one bounded response, preserving errors for the retry chain.
pub fn generate(
    prompt: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    timeout: Option<RequestTimeouts>,
) -> Result<String> {
    generate_with(
        &UREQ_RESPONSE_AGENT,
        ENDPOINT,
        prompt,
        cancel_token,
        timeout,
    )
}

fn check_cancelled(cancel_token: &Option<Arc<AtomicBool>>) -> Result<()> {
    if cancel_token
        .as_ref()
        .is_some_and(|token| token.load(Ordering::Relaxed))
    {
        bail!("Cancelled");
    }
    Ok(())
}

fn generate_with(
    agent: &ureq::Agent,
    endpoint: &str,
    prompt: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    timeout: Option<RequestTimeouts>,
) -> Result<String> {
    check_cancelled(cancel_token)?;
    let payload = serde_json::json!({
        "messages": [{ "role": "user", "content": prompt }],
        "chatOptions": { "selectedModel": MODEL, "topK": TOP_K }
    });
    let request = agent
        .post(endpoint)
        .header("Content-Type", "application/json");
    #[cfg(not(feature = "recorder-worker"))]
    let request = crate::api::client::with_request_timeouts(request, timeout);
    #[cfg(feature = "recorder-worker")]
    let request = match timeout {
        Some(deadline) => request.config().timeout_global(Some(deadline)).build(),
        None => request,
    };
    let response = request.send_json(payload);
    check_cancelled(cancel_token)?;
    let response = response.map_err(|error| anyhow::anyhow!(transport_error(&error)))?;
    check_cancelled(cancel_token)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| format!("; retry-after: {seconds}"))
            .unwrap_or_default();
        let mut body = Vec::new();
        let _ = response
            .into_body()
            .into_reader()
            .take(MAX_ERROR_BYTES)
            .read_to_end(&mut body);
        check_cancelled(cancel_token)?;
        bail!(
            "Taalas HTTP {status}{retry_after}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    read_response(response.into_body().into_reader(), cancel_token)
}

#[cfg(not(feature = "recorder-worker"))]
fn transport_error(error: &ureq::Error) -> String {
    crate::api::client::transport_error_message("Taalas transport error", error)
}

#[cfg(feature = "recorder-worker")]
fn transport_error(error: &ureq::Error) -> String {
    format!("Taalas transport error: {error}")
}

fn read_response(mut reader: impl Read, cancel_token: &Option<Arc<AtomicBool>>) -> Result<String> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        check_cancelled(cancel_token)?;
        let result = reader.read(&mut buf);
        check_cancelled(cancel_token)?;
        let n = result.map_err(|error| {
            anyhow::anyhow!("PROVIDER_RESPONSE_INVALID:Taalas response read error: {error}")
        })?;
        if n == 0 {
            break;
        }
        if n > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
            bail!("PROVIDER_RESPONSE_INVALID:Taalas response exceeds size limit");
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    let raw = String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("PROVIDER_RESPONSE_INVALID:Taalas response is not UTF-8"))?;
    let clean = raw
        .split_once(STATS_MARKER)
        .map_or(raw.as_str(), |(text, _)| text)
        .trim();
    if clean.is_empty() {
        bail!("EMPTY_MODEL_RESPONSE:taalas");
    }
    Ok(clean.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    struct ByteReader<'a>(&'a [u8]);
    impl Read for ByteReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    #[test]
    fn decodes_codepoints_split_across_reads_and_strips_stats() {
        let bytes = " 한국어 tiếng Việt <|stats|>ignored".as_bytes();
        assert_eq!(
            read_response(ByteReader(bytes), &None).unwrap(),
            "한국어 tiếng Việt"
        );
    }

    #[test]
    fn rejects_invalid_empty_and_oversized_responses() {
        for bytes in [&[0xff][..], b"  <|stats|>anything".as_slice()] {
            let err = read_response(bytes, &None).unwrap_err().to_string();
            assert!(
                err.starts_with("PROVIDER_RESPONSE_INVALID:")
                    || err.starts_with("EMPTY_MODEL_RESPONSE:")
            );
        }
        let oversized = vec![b'x'; MAX_RESPONSE_BYTES + 1];
        assert!(
            read_response(oversized.as_slice(), &None)
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
    }

    #[test]
    fn cancellation_before_and_during_read_discards_partial_output() {
        struct CancellingReader(Arc<AtomicBool>);
        impl Read for CancellingReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                buf[0] = b'x';
                self.0.store(true, Ordering::Relaxed);
                Ok(1)
            }
        }
        let token = Arc::new(AtomicBool::new(true));
        assert_eq!(
            read_response(&b"text"[..], &Some(token.clone()))
                .unwrap_err()
                .to_string(),
            "Cancelled"
        );
        token.store(false, Ordering::Relaxed);
        assert_eq!(
            read_response(CancellingReader(token.clone()), &Some(token))
                .unwrap_err()
                .to_string(),
            "Cancelled"
        );
    }

    #[test]
    fn http_failure_keeps_status_and_retry_delay_for_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            let (body_start, body_len) = loop {
                let read = stream.read(&mut buf).unwrap();
                assert!(read > 0, "request ended before headers");
                request.extend_from_slice(&buf[..read]);
                assert!(request.len() <= 16 * 1024, "request headers too large");
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&request[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .expect("request must have a content length");
                    break (end + 4, length);
                }
            };
            while request.len() - body_start < body_len {
                let read = stream.read(&mut buf).unwrap();
                assert!(read > 0, "request ended before body");
                request.extend_from_slice(&buf[..read]);
            }
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 4\r\nRetry-After: 12\r\nConnection: close\r\n\r\nbusy").unwrap();
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let error = generate_with(&agent, &format!("http://{address}/"), "test", &None, None)
            .unwrap_err()
            .to_string();
        server.join().unwrap();
        assert!(error.contains("HTTP 429"), "unexpected error: {error}");
        assert!(error.contains("retry-after: 12"));
        #[cfg(not(feature = "recorder-worker"))]
        assert!(crate::overlay::utils::should_advance_retry_chain(&error));
    }

    #[test]
    fn a_deadline_bounds_a_silent_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let _ = released.recv_timeout(Duration::from_secs(30));
            drop(stream);
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        #[cfg(not(feature = "recorder-worker"))]
        let deadline = RequestTimeouts::uniform(Duration::from_millis(500));
        #[cfg(feature = "recorder-worker")]
        let deadline = Duration::from_millis(500);
        let started = std::time::Instant::now();
        let result = generate_with(
            &agent,
            &format!("http://{address}/"),
            "test",
            &None,
            Some(deadline),
        );
        let elapsed = started.elapsed();
        let _ = release.send(());
        server.join().unwrap();
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(10), "waited {elapsed:?}");
    }
}
