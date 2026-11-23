use std::io::Write;
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, RwLock};

use crate::loopback_resource::{self as transport, Connection, Limits};
type Html = Arc<RwLock<Option<Arc<str>>>>;

pub(super) struct ResourceServer {
    origin: String,
    html: Html,
    _transport: transport::Server,
}

impl ResourceServer {
    pub(super) fn origin(&self) -> &str {
        &self.origin
    }
    pub(super) fn set_html(&self, html: String) {
        *self.html.write().unwrap() = Some(Arc::from(html));
    }
}

pub(super) fn start() -> anyhow::Result<ResourceServer> {
    start_with_limits(Limits::default())
}

fn start_with_limits(limits: Limits) -> anyhow::Result<ResourceServer> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| anyhow::anyhow!("generate compositor resource token: {error}"))?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let server_token = token.clone();
    let html: Html = Arc::new(RwLock::new(None));
    let server_html = html.clone();
    let transport = transport::Server::start(listener, limits, move |stream| {
        handle(stream, &server_token, &server_html)
    })?;
    Ok(ResourceServer {
        origin: format!("http://{address}/{token}"),
        html,
        _transport: transport,
    })
}

fn handle(stream: &mut Connection, token: &str, html: &Html) {
    let mut request = [0_u8; 8192];
    let Ok(length) = transport::read_http_header(stream, &mut request) else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..length]);
    let expected_host = match stream.local_addr() {
        Ok(addr) => addr.to_string(),
        Err(_) => return,
    };
    let mut hosts = request.lines().skip(1).filter_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host").then_some(value.trim())
    });
    if hosts.next() != Some(expected_host.as_str()) || hosts.next().is_some() {
        write_response(stream, "GET", 404, "text/plain", b"Not Found", "no-store");
        return;
    }
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

    let Some(path) = path.strip_prefix(&format!("/{token}/")) else {
        write_response(stream, method, 404, "text/plain", b"Not Found", "no-store");
        return;
    };

    if method != "GET" && method != "HEAD" {
        write_response(stream, method, 404, "text/plain", b"Not Found", "no-store");
        return;
    }
    let document = if path == "index.html" {
        html.read().unwrap().clone()
    } else {
        None
    };
    if let Some(html) = document {
        write_response(
            stream,
            method,
            200,
            "text/html; charset=utf-8",
            html.as_bytes(),
            "no-store",
        );
        return;
    }
    if path == "font.woff2" {
        write_response(
            stream,
            method,
            200,
            "font/woff2",
            super::font::bytes(),
            "public, max-age=31536000, immutable",
        );
        return;
    }
    write_response(
        stream,
        method,
        404,
        "text/plain; charset=utf-8",
        b"Not Found",
        "no-store",
    );
}

fn write_response(
    stream: &mut Connection,
    method: &str,
    status: u16,
    content_type: &str,
    body: &[u8],
    cache_control: &str,
) {
    let status_text = match status {
        200 => "OK",
        _ => "Not Found",
    };
    let headers = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: {cache_control}\r\n\
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    #[test]
    fn origins_keep_independent_documents_and_shutdown_ownership() {
        let first = start().unwrap();
        let second = start().unwrap();
        first.set_html("first document".into());
        second.set_html("second document".into());
        let first_url = format!("{}/index.html", first.origin());
        let second_url = format!("{}/index.html", second.origin());
        assert_eq!(
            ureq::get(&first_url)
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap(),
            "first document"
        );
        assert_eq!(
            ureq::get(&second_url)
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap(),
            "second document"
        );
        first.set_html("updated first document".into());
        assert_eq!(
            ureq::get(&first_url)
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap(),
            "updated first document"
        );
        drop(first);
        assert!(ureq::get(&first_url).call().is_err());
        assert_eq!(
            ureq::get(&second_url)
                .call()
                .unwrap()
                .into_body()
                .read_to_string()
                .unwrap(),
            "second document"
        );
    }

    #[test]
    fn incomplete_http_header_cannot_pin_a_worker_by_trickling() {
        let server = start_with_limits(Limits {
            timeout: Duration::from_millis(100),
            ..Limits::default()
        })
        .unwrap();
        server.set_html("document".into());
        let address = server
            .origin()
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut writer = client.try_clone().unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..20 {
                if writer.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let started = Instant::now();
        let mut response = Vec::new();
        let result = client.read_to_end(&mut response);
        assert!(
            result.is_ok()
                || result.is_err_and(|error| matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ))
        );
        assert!(response.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(client);
        sender.join().unwrap();
    }

    #[test]
    fn oversized_header_is_closed_without_serving_a_document() {
        let server = start().unwrap();
        server.set_html("document".into());
        let address = server
            .origin()
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        // Longer than the server's absolute connection budget, so a loaded
        // test host cannot time the client out before the server closes.
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        client.write_all(&vec![b'x'; 8192]).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert!(response.is_empty());
    }

    #[test]
    fn isolated_server_serves_html_and_font() {
        let server = start().expect("server should start");
        server.set_html("<html><body>test</body></html>".to_string());
        let origin = server.origin().to_owned();

        let html_resp = ureq::get(&format!("{origin}/index.html")).call().unwrap();
        assert_eq!(html_resp.status(), 200);
        assert_eq!(
            html_resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/html; charset=utf-8")
        );
        let mut html_body = String::new();
        html_resp
            .into_body()
            .into_reader()
            .read_to_string(&mut html_body)
            .unwrap();
        assert_eq!(html_body, "<html><body>test</body></html>");

        let font_resp = ureq::get(&format!("{origin}/font.woff2")).call().unwrap();
        assert_eq!(font_resp.status(), 200);
        assert_eq!(
            font_resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("font/woff2")
        );
        let mut font_bytes = Vec::new();
        font_resp
            .into_body()
            .into_reader()
            .read_to_end(&mut font_bytes)
            .unwrap();
        assert_eq!(font_bytes, super::super::font::bytes());
    }

    fn raw_response(origin: &str, parts: &[&str]) -> String {
        let address = origin
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let mut stream = TcpStream::connect(address).unwrap();
        for part in parts {
            stream.write_all(part.as_bytes()).unwrap();
        }
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn server_rejects_card_paths_and_untrusted_hosts_without_cross_origin_grants() {
        let server = start().unwrap();
        server.set_html("<html><body>test</body></html>".into());
        let origin = server.origin().to_owned();
        let (host, token) = origin
            .strip_prefix("http://")
            .unwrap()
            .split_once('/')
            .unwrap();
        let card = raw_response(
            &origin,
            &[&format!(
                "GET /{token}/card/123 HTTP/1.1\r\nHost: {host}\r\n\r\n"
            )],
        );
        assert!(card.starts_with("HTTP/1.1 404"));
        assert!(!card.to_ascii_lowercase().contains("access-control-allow-"));
        let rebinding = raw_response(
            &origin,
            &[&format!(
                "GET /{token}/index.html HTTP/1.1\r\nHost: attacker.example\r\n\r\n"
            )],
        );
        assert!(rebinding.starts_with("HTTP/1.1 404"));
        let duplicate = raw_response(
            &origin,
            &[&format!(
                "GET /{token}/index.html HTTP/1.1\r\nHost: {host}\r\nHost: attacker.example\r\n\r\n"
            )],
        );
        assert!(duplicate.starts_with("HTTP/1.1 404"));
        let split = raw_response(
            &origin,
            &[
                &format!("GET /{token}/index.html HTTP/1.1\r\n"),
                &format!("Host: {host}\r\n\r\n"),
            ],
        );
        assert!(split.starts_with("HTTP/1.1 200"));
        let preflight = raw_response(
            &origin,
            &[&format!(
                "OPTIONS /{token}/index.html HTTP/1.1\r\nHost: {host}\r\n\r\n"
            )],
        );
        assert!(preflight.starts_with("HTTP/1.1 404"));
        let missing_token = raw_response(
            &origin,
            &[&format!("GET /index.html HTTP/1.1\r\nHost: {host}\r\n\r\n")],
        );
        assert!(missing_token.starts_with("HTTP/1.1 404"));
        assert!(
            !preflight
                .to_ascii_lowercase()
                .contains("access-control-allow-")
        );
    }
}
