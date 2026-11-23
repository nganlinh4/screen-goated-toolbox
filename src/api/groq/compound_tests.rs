use super::*;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::time::Instant;

/// Serves one compound completion that ran a search with two sources.
fn compound_server() -> (String, std::thread::JoinHandle<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
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
        let body = serde_json::json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "synthetic answer",
                    "executed_tools": [{
                        "type": "search",
                        "arguments": "{\"query\":\"synthetic query\"}",
                        "search_results": {"results": [
                            {"title": "Low", "url": "https://low.example/a", "score": 0.2, "content": "x"},
                            {"title": "High", "url": "https://high.example/b", "score": 0.9, "content": "y"}
                        ]}
                    }]
                }
            }]
        })
        .to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        serde_json::from_slice(&request).unwrap()
    });
    (endpoint, server)
}

fn request<'a>(
    endpoint: &'a str,
    cancel_token: &'a Option<Arc<AtomicBool>>,
) -> CompoundRequest<'a> {
    CompoundRequest {
        endpoint,
        api_key: "synthetic-key",
        model: "groq/compound",
        prompt: "synthetic prompt",
        search_label: None,
        ui_language: "en",
        request_timeout: None,
        cancel_token,
    }
}

#[test]
fn compound_phases_replace_each_other_and_stop_waits_end_at_once() {
    let mut shown = Vec::new();
    show_phase(&mut |chunk: &str| shown.push(chunk.to_string()), "phase");
    assert_eq!(shown, [format!("{}phase", crate::api::WIPE_SIGNAL)]);

    let started = Instant::now();
    let stopped = Some(Arc::new(AtomicBool::new(true)));
    assert!(hold_phase(Duration::from_secs(30), &stopped).is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn search_progress_replaces_itself_and_ends_with_the_answer() {
    let (endpoint, server) = compound_server();
    let mut chunks = Vec::new();
    let answer = run_compound(request(&endpoint, &None), |chunk| {
        chunks.push(chunk.to_string())
    })
    .unwrap();
    let sent = server.join().unwrap();

    assert_eq!(answer, "synthetic answer");
    assert_eq!(sent["max_completion_tokens"], 8192);
    assert_eq!(
        sent["compound_custom"]["tools"]["enabled_tools"],
        serde_json::json!(["web_search", "visit_website"])
    );
    assert_eq!(chunks.len(), 5, "{chunks:?}");
    assert!(
        chunks[1..]
            .iter()
            .all(|chunk| chunk.starts_with(crate::api::WIPE_SIGNAL))
    );
    assert!(chunks[1].contains("synthetic query"));
    assert!(chunks[2].find("high.example") < chunks[2].find("low.example"));
    assert_eq!(
        chunks[4],
        format!("{}synthetic answer", crate::api::WIPE_SIGNAL)
    );
}

#[test]
fn stop_during_search_progress_ends_the_request() {
    let (endpoint, server) = compound_server();
    let cancel_token = Some(Arc::new(AtomicBool::new(false)));
    let flag = cancel_token.clone().unwrap();
    let started = Instant::now();
    let result = run_compound(request(&endpoint, &cancel_token), |chunk| {
        if chunk.starts_with(crate::api::WIPE_SIGNAL) {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
    server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(started.elapsed() < Duration::from_millis(700));
}
