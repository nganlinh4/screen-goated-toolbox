use super::ollama_generate_text_cancellable;
#[cfg(not(feature = "recorder-worker"))]
use super::{OllamaVisionRequest, ollama_generate_vision};
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[test]
fn cancellation_stops_later_stream_chunks() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("local request did not arrive: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(&mut stream);
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
        let body = "{\"thinking\":\"pending\",\"response\":\"first\",\"done\":false}\n{\"response\":\"late\",\"done\":true}\n";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let cancelled = AtomicBool::new(false);
    let mut chunks = Vec::new();
    let result = ollama_generate_text_cancellable(
        &endpoint,
        "local-model",
        "synthetic input",
        true,
        "en",
        Some(&cancelled),
        |chunk| {
            chunks.push(chunk.to_string());
            cancelled.store(true, Ordering::SeqCst);
        },
    );
    server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(chunks.len(), 1);
    assert!(
        !chunks
            .iter()
            .any(|chunk| chunk.contains("first") || chunk.contains("late"))
    );
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn canceled_vision_request_never_opens_a_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let cancelled = AtomicBool::new(true);
    let result = ollama_generate_vision(
        OllamaVisionRequest {
            base_url: &endpoint,
            model: "local-model",
            prompt: "synthetic input",
            image: image::RgbaImage::new(1, 1),
            streaming_enabled: false,
            ui_language: "en",
            cancel_token: Some(&cancelled),
        },
        |_| panic!("canceled request emitted a chunk"),
    );
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
