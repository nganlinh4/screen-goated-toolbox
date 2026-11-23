use super::{PAGES, serve_product_font};
use crate::loopback_resource::{self, Connection, Limits, Server};
use std::io::{self, Write};
use std::net::{Shutdown, TcpListener};
use std::time::{Duration, Instant};

const CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\n\
    Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n\
    Access-Control-Allow-Headers: *\r\n\
    Access-Control-Allow-Private-Network: true\r\n";

pub(super) struct PageServer {
    origin: String,
    transport: Server,
}

impl PageServer {
    pub(super) fn start() -> anyhow::Result<Self> {
        Self::start_with_limits(Limits {
            workers: 8,
            queue: 64,
            timeout: Duration::from_secs(2),
        })
    }

    pub(super) fn start_with_limits(limits: Limits) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let origin = format!("http://{}", listener.local_addr()?);
        let transport = Server::start(listener, limits, |stream| {
            let _ = handle_request(stream);
        })?;
        Ok(Self { origin, transport })
    }

    pub(super) fn origin(&self) -> &str {
        &self.origin
    }
    pub(super) fn is_running(&self) -> bool {
        self.transport.is_running()
    }
}

fn handle_request(stream: &mut Connection) -> io::Result<()> {
    let mut buffer = [0_u8; 8192];
    let length = loopback_resource::read_http_header(stream, &mut buffer)?;
    let request = std::str::from_utf8(&buffer[..length])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid resource HTTP header"))?;
    let mut lines = request.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return write_response(stream, method, 400, "text/plain", b"Invalid request");
    }
    let expected_host = stream.local_addr()?.to_string();
    let mut hosts = lines.filter_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host").then_some(value.trim())
    });
    if hosts.next() != Some(expected_host.as_str()) || hosts.next().is_some() {
        return write_response(stream, method, 404, "text/plain", b"Not Found");
    }
    if method == "OPTIONS" {
        return write_response(stream, method, 204, "text/plain", b"");
    }
    if !matches!(method, "GET" | "HEAD") {
        return write_response(stream, method, 405, "text/plain", b"Method not allowed");
    }
    let path = path.split('?').next().unwrap_or(path);
    if serve_product_font(stream, method, path, CORS_HEADERS)? {
        return Ok(());
    }
    if let Some(id) = path.strip_prefix("/page/") {
        let html = PAGES
            .lock()
            .ok()
            .and_then(|mut pages| pages.get(id, Instant::now()));
        return match html {
            Some(html) => write_response(
                stream,
                method,
                200,
                "text/html; charset=utf-8",
                html.as_bytes(),
            ),
            None => write_response(stream, method, 404, "text/plain", b"Page not found"),
        };
    }
    write_response(stream, method, 404, "text/plain", b"Not Found")
}

fn write_response(
    stream: &mut Connection,
    method: &str,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        405 => "Method Not Allowed",
        _ => "Not Found",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{CORS_HEADERS}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(body)?;
    }
    stream.flush()?;
    stream.shutdown(Shutdown::Write)
}
