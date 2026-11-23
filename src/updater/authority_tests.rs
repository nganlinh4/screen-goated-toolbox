use super::*;
use serde_json::Value;
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::time::Instant;

fn fixture() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/app-update/authority.json"
    )))
    .unwrap()
}

fn bytes(value: &Value, field: &str) -> Vec<u8> {
    crate::crypto::decode_hex(value[field].as_str().unwrap(), "fixture").unwrap()
}

#[test]
fn signatures_bind_strictly_encoded_manifest_bytes() {
    let fixture = fixture();
    let key = bytes(&fixture, "public_key_hex");
    for (name, vector) in fixture["vectors"].as_object().unwrap() {
        let payload = bytes(vector, "payload_hex");
        let signature = bytes(vector, "signature_hex");
        let result = crate::crypto::verify_p256_sha256(&key, &payload, &signature, "fixture")
            .and_then(|()| parse(&payload));
        assert_eq!(
            result.is_ok(),
            vector["accepted"].as_bool().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn only_missing_primary_allows_fallback_through_http_and_verification() {
    let fixture = fixture();
    for case in fixture["http_cases"].as_array().unwrap() {
        // Fallback parser behavior is covered separately; this test owns the
        // primary HTTP boundary and the decision to invoke fallback.
        if case.get("fallback_mode").is_some() {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = case["requests"].as_array().unwrap();
        let expected: Vec<_> = requests
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| *name != "fallback")
            .map(str::to_owned)
            .collect();
        let expected_server = expected.clone();
        let vector = fixture["vectors"][case["vector"].as_str().unwrap()].clone();
        let case_server = case.clone();
        let server = std::thread::spawn(move || {
            let mut observed = Vec::new();
            for resource in expected_server {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("owned update request did not arrive: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert!(first.starts_with(&format!("GET /{resource} ")), "{first}");
                let mut total = first.len();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    total += line.len();
                    assert!(total < 16 * 1024);
                    if line == "\r\n" {
                        break;
                    }
                }
                observed.push(resource.clone());
                let status = case_server[format!("{resource}_status")].as_u64().unwrap();
                if status == 0 {
                    stream.write_all(b"HTTP/1.1 invalid\r\n\r\n").unwrap();
                    continue;
                }
                let mut body = bytes(
                    &vector,
                    if resource == "manifest" {
                        "payload_hex"
                    } else {
                        "signature_hex"
                    },
                );
                let mode = case_server["body_mode"].as_str().unwrap_or("");
                if resource == "manifest" && mode.starts_with("manifest_") {
                    body = vec![b'x'; MAX_MANIFEST_BYTES as usize + 1];
                } else if resource == "signature" && mode == "signature_oversize" {
                    body.push(0);
                } else if resource == "signature" && mode == "short_signature" {
                    body.pop();
                }
                let chunked = mode == "manifest_oversize" || mode == "signature_oversize";
                let mut response =
                    format!("HTTP/1.1 {status} Test\r\nConnection: close\r\n").into_bytes();
                if chunked {
                    response.extend_from_slice(
                        format!("Transfer-Encoding: chunked\r\n\r\n{:x}\r\n", body.len())
                            .as_bytes(),
                    );
                    response.extend_from_slice(&body);
                    response.extend_from_slice(b"\r\n0\r\n\r\n");
                } else {
                    response.extend_from_slice(
                        format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes(),
                    );
                    response.extend_from_slice(&body);
                }
                // A bounded reader may close before the oversized body finishes.
                let _ = stream.write_all(&response);
            }
            observed
        });
        let mut fallback_calls = 0;
        let result = crate::updater::latest_candidate_with(
            || {
                fetch_with(
                    &format!("{endpoint}/manifest"),
                    &format!("{endpoint}/signature"),
                    || match case["key_mode"].as_str() {
                        Some("public_key_unavailable") => anyhow::bail!("owned key read failure"),
                        Some("public_key_invalid") => Ok(vec![0; 65]),
                        _ => Ok(bytes(&fixture, "public_key_hex")),
                    },
                )
            },
            || {
                fallback_calls += 1;
                parse(&bytes(&fixture["vectors"]["valid"], "payload_hex"))
            },
        );
        assert_eq!(server.join().unwrap(), expected, "{name}");
        let outcome = case["expected"].as_str().unwrap();
        assert_eq!(fallback_calls, usize::from(outcome == "fallback"), "{name}");
        assert_eq!(result.is_ok(), outcome != "error", "{name}: {result:?}");
    }
}
