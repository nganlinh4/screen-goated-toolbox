//! Loopback HTTP fixtures for provider request tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

/// Serves one scripted HTTP response per connection and returns each request body.
pub(crate) fn scripted_server(
    responses: Vec<&'static str>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut bodies = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            let (body_start, length) = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "request ended before headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .map(|value| value.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (end + 4, length);
                }
            };
            while request.len() - body_start < length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "request ended before body");
                request.extend_from_slice(&buffer[..read]);
            }
            bodies.push(String::from_utf8_lossy(&request[body_start..]).into_owned());
            stream.write_all(response.as_bytes()).unwrap();
        }
        bodies
    });
    (endpoint, server)
}
