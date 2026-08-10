use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};

pub(super) fn start() -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    std::thread::Builder::new()
        .name("result-card-origin".to_string())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = std::thread::Builder::new()
                    .name("result-card-resource".to_string())
                    .spawn(move || handle(stream));
            }
        })?;
    Ok(format!("http://{address}"))
}

fn handle(mut stream: TcpStream) {
    let mut request = [0_u8; 8192];
    let Ok(length) = stream.read(&mut request) else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..length]);
    let mut request_parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = request_parts.next().unwrap_or("GET");
    let path = request_parts
        .next()
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");

    if method == "OPTIONS" {
        write_response(&mut stream, method, 204, "text/plain", b"", "no-store");
        return;
    }
    if let Some(id) = path
        .strip_prefix("/card/")
        .and_then(|value| value.parse::<isize>().ok())
    {
        if let Some(document) = super::child::CARDS
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|card| card.document.clone())
        {
            crate::debug_log::log_debug(&format!(
                "[ResultCompositor] resource=isolated_document action=served id={id} bytes={}",
                document.len()
            ));
            write_response(
                &mut stream,
                method,
                200,
                "text/html; charset=utf-8",
                document.as_bytes(),
                "no-store",
            );
            return;
        }
        crate::debug_log::log_debug(&format!(
            "[ResultCompositor] resource=isolated_document action=missing id={id}"
        ));
    }
    write_response(
        &mut stream,
        method,
        404,
        "text/plain; charset=utf-8",
        b"Not Found",
        "no-store",
    );
}

fn write_response(
    stream: &mut TcpStream,
    method: &str,
    status: u16,
    content_type: &str,
    body: &[u8],
    cache_control: &str,
) {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        _ => "Not Found",
    };
    let headers = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: {cache_control}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n\
         Access-Control-Allow-Private-Network: true\r\n\
         Cross-Origin-Resource-Policy: cross-origin\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    if method != "HEAD" {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}
