//! Chain deadlines for Ollama streams (host only; the recorder worker has no
//! request-scoped deadlines).
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Accepts one request, answers with streaming headers and then stalls until
/// `release` fires, like a local model that is still loading.
fn stalled_stream_server() -> (
    String,
    std::sync::mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (release, released) = std::sync::mpsc::channel::<()>();
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
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    });
    (endpoint, release, server)
}

fn chain_request<'a>(
    endpoint: &'a str,
    cancel_token: &'a Option<std::sync::Arc<AtomicBool>>,
    first_token: Duration,
) -> super::OllamaChainTextRequest<'a> {
    super::OllamaChainTextRequest {
        base_url: endpoint,
        model: "local-model",
        prompt: "synthetic input",
        streaming_enabled: true,
        ui_language: "en",
        cancel_token,
        request_timeout: Some(crate::api::client::RequestTimeouts {
            first_token,
            ..crate::api::client::RequestTimeouts::uniform(Duration::from_secs(30))
        }),
    }
}

#[test]
fn stalled_local_stream_returns_at_the_chain_first_output_deadline() {
    let (endpoint, release, server) = stalled_stream_server();
    let started = Instant::now();
    let result = super::ollama_generate_text_in_chain(
        chain_request(&endpoint, &None, Duration::from_millis(400)),
        |_| panic!("a stalled stream emitted a chunk"),
    );
    let elapsed = started.elapsed();
    release.send(()).unwrap();
    server.join().unwrap();
    assert!(result.is_err());
    assert!(elapsed < Duration::from_secs(4), "took {elapsed:?}");
}

#[test]
fn cancel_interrupts_a_blocked_local_stream_read() {
    let (endpoint, release, server) = stalled_stream_server();
    let cancel_token = Some(std::sync::Arc::new(AtomicBool::new(false)));
    let flag = cancel_token.clone().unwrap();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let result = super::ollama_generate_text_in_chain(
        chain_request(&endpoint, &cancel_token, Duration::from_secs(30)),
        |_| panic!("a stalled stream emitted a chunk"),
    );
    let elapsed = started.elapsed();
    canceller.join().unwrap();
    release.send(()).unwrap();
    server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(elapsed < Duration::from_secs(4), "took {elapsed:?}");
}
