//! The recorder worker must answer provider questions exactly like the host
//! app. These tests drive the worker's own entry points.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Serves `/api/tags` and `/api/show` like an Ollama server that reports no
/// vision family, so the model name alone decides the capability.
fn serve_ollama(model: &'static str, requests: usize) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..requests {
            let mut stream = accept(&listener);
            let path = read_request_path(&mut stream);
            let body = if path.ends_with("/api/tags") {
                format!("{{\"models\":[{{\"name\":\"{model}\"}}]}}")
            } else {
                "{\"modelfile\":\"FROM base\",\"details\":{\"families\":[\"llama\"]}}".to_string()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (endpoint, server)
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("local request did not arrive: {error}"),
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
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
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn worker_lists_glued_vl_ollama_models_as_vision_like_the_host() {
    let (endpoint, server) = serve_ollama("internvl3:8b", 2);
    let models = super::ollama::fetch_ollama_models_with_caps(&endpoint).unwrap();
    server.join().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "internvl3:8b");
    assert!(
        models[0].has_vision,
        "the host lists internvl3 as vision-capable, so the worker must too"
    );
}
