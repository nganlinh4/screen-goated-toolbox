use super::*;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
fn real_http_transport_retries_declared_output_allowance_for_both_response_modes() {
    for streaming in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/completions", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut limits = Vec::new();
            while limits.len() < 2 {
                let (socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("request did not arrive: {error}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(socket);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                limits.push(request["max_completion_tokens"].as_u64().unwrap());
                let (status, body) = if limits.len() == 1 {
                    ("429 Too Many Requests", r#"{"error":{"code":"rate_limit_exceeded","type":"tokens","message":"(OTPM): Limit 1000, Requested 2048. Reduce output allowance."}}"#.to_string())
                } else if streaming {
                    ("200 OK", "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ready\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into())
                } else {
                    (
                        "200 OK",
                        r#"{"choices":[{"message":{"content":"ready"},"finish_reason":"stop"}]}"#
                            .into(),
                    )
                };
                write!(
                    reader.get_mut(),
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            limits
        });
        let payload = serde_json::json!({"model":"test-endpoint","max_completion_tokens":2048,"stream":streaming});
        let result = send_standard_payload_to(
            &endpoint,
            "synthetic-key",
            &payload,
            TranslateTransportOptions {
                locally_validated_schema: streaming,
                max_output_tokens: Some(2048),
                streaming_enabled: streaming,
                ui_language: "en",
                cancel_token: &None,
                request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                    Duration::from_secs(3),
                )),
                search_enabled: false,
            },
            |_| {},
        );
        assert_eq!(server.join().unwrap(), [2048, 1000]);
        assert_eq!(result.unwrap(), "ready");
        assert_eq!(payload["max_completion_tokens"], 2048);
    }
}

#[test]
fn generic_json_content_is_unchanged_in_both_response_modes() {
    let content = r#"{"authorized":true,"translation":"literal field"}"#;
    for streaming in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/completions", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (socket, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("request did not arrive: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = std::io::BufReader::new(socket);
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(
                    reader.read_line(&mut line).unwrap(),
                    0,
                    "request closed before headers completed"
                );
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).unwrap();
            let body = if streaming {
                let event = serde_json::json!({"choices":[{"index":0,"delta":{"content":content},"finish_reason":"stop"}]});
                format!("data: {event}\n\ndata: [DONE]\n\n")
            } else {
                serde_json::json!({"choices":[{"message":{"content":content},"finish_reason":"stop"}]}).to_string()
            };
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let payload = standard_payload(
            "openai/gpt-oss-20b",
            "return JSON",
            true,
            None,
            TranslateTransportOptions {
                locally_validated_schema: false,
                max_output_tokens: None,
                streaming_enabled: streaming,
                ui_language: "en",
                cancel_token: &None,
                request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                    Duration::from_secs(3),
                )),
                search_enabled: false,
            },
        );
        let mut emitted = String::new();
        let actual = send_standard_payload_to(
            &endpoint,
            "synthetic-key",
            &payload,
            TranslateTransportOptions {
                locally_validated_schema: false,
                max_output_tokens: None,
                streaming_enabled: streaming,
                ui_language: "en",
                cancel_token: &None,
                request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                    Duration::from_secs(3),
                )),
                search_enabled: false,
            },
            |chunk| emitted.push_str(chunk),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(actual, content);
        assert_eq!(emitted, content);
    }
}
