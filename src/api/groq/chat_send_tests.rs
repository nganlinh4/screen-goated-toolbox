use super::*;
use crate::api::http_test_support::scripted_server;

fn send(
    endpoint: &str,
    retry_short_rate_limit: bool,
    retries: &mut Vec<Duration>,
) -> Result<String> {
    let mut payload = serde_json::json!({"model": "fixture-model", "messages": []});
    send_chat(
        ChatSend {
            endpoint,
            api_key: "fixture-key",
            request_timeout: None,
            cancel_token: &None,
            retry_short_rate_limit,
            transport_label: "Groq test transport error",
        },
        &mut payload,
        |delay| retries.push(delay),
    )
    .map(|response| response.into_body().read_to_string().unwrap())
}

const RATE_LIMITED: &str = "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";

#[test]
fn one_short_rate_limit_is_waited_out_in_place_when_allowed() {
    let (endpoint, server) = scripted_server(vec![RATE_LIMITED, OK]);
    let mut retries = Vec::new();
    assert_eq!(
        send(&endpoint, true, &mut retries).unwrap(),
        "{\"ok\":true}"
    );
    assert_eq!(retries, [Duration::from_secs(1)]);
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn rate_limit_reaches_the_chain_when_in_place_retry_is_off() {
    let (endpoint, server) = scripted_server(vec![RATE_LIMITED]);
    let mut retries = Vec::new();
    let error = send(&endpoint, false, &mut retries)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Groq API HTTP 429"), "{error}");
    assert!(retries.is_empty());
    server.join().unwrap();
}

#[test]
fn rejected_key_maps_to_the_shared_invalid_key_signal() {
    let (endpoint, server) = scripted_server(vec![
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    ]);
    let error = send(&endpoint, true, &mut Vec::new())
        .unwrap_err()
        .to_string();
    assert_eq!(error, "INVALID_API_KEY:groq");
    server.join().unwrap();
}
