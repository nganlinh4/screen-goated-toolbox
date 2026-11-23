use super::*;
use crate::loopback_resource::Limits;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn page_fixture(timeout: Duration) -> (server::PageServer, String) {
    let server = server::PageServer::start_with_limits(Limits {
        workers: 2,
        queue: 4,
        timeout,
    })
    .unwrap();
    let html = "bounded page \u{1f9ea}";
    let id = format!("{:x}", Sha256::digest(html.as_bytes()));
    assert!(
        PAGES
            .lock()
            .unwrap()
            .insert(id.clone(), html.into(), Instant::now())
    );
    (server, format!("/page/{id}"))
}

fn request_response(server: &server::PageServer, request: &[u8]) -> Vec<u8> {
    let address = server.origin().strip_prefix("http://").unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client.write_all(request).unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    response
}

#[test]
fn split_http_headers_wait_for_completion_then_serve_exact_bytes() {
    let (server, path) = page_fixture(Duration::from_secs(1));
    let address = server.origin().strip_prefix("http://").unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    write!(client, "GET {path} HTTP/1.1\r\n").unwrap();
    let error = client.read(&mut [0_u8; 1]).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ));
    write!(client, "Host: {address}\r\n\r\n").unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.ends_with("bounded page \u{1f9ea}"));
}

#[test]
fn incomplete_and_oversized_headers_never_serve_a_page() {
    let (server, path) = page_fixture(Duration::from_millis(100));
    let address = server.origin().strip_prefix("http://").unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(client, "GET {path} HTTP/1.1\r\n").unwrap();
    let mut writer = client.try_clone().unwrap();
    let sender = std::thread::spawn(move || {
        for _ in 0..20 {
            if writer.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
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
    drop(client);
    sender.join().unwrap();
    assert!(request_response(&server, &[b'x'; 8192]).is_empty());
}

#[test]
fn head_and_preflight_have_no_body_and_unsupported_methods_do_not_read_pages() {
    let (server, path) = page_fixture(Duration::from_secs(1));
    let address = server.origin().strip_prefix("http://").unwrap();
    let head = request_response(
        &server,
        format!("HEAD {path} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
    );
    let head = String::from_utf8(head).unwrap();
    assert!(head.starts_with("HTTP/1.1 200 OK"));
    assert!(head.contains(&format!(
        "Content-Length: {}\r\n",
        "bounded page \u{1f9ea}".len()
    )));
    assert!(head.ends_with("\r\n\r\n"));
    let options = request_response(
        &server,
        format!("OPTIONS {path} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
    );
    assert!(options.starts_with(b"HTTP/1.1 204 No Content"));
    assert!(options.ends_with(b"\r\n\r\n"));
    let post = request_response(
        &server,
        format!("POST {path} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
    );
    assert!(post.starts_with(b"HTTP/1.1 405"));
    assert!(!String::from_utf8(post).unwrap().contains("bounded page"));
}

#[test]
fn authority_is_exact_and_body_bytes_cannot_become_headers() {
    let (server, path) = page_fixture(Duration::from_secs(1));
    let address = server.origin().strip_prefix("http://").unwrap();
    for headers in [
        String::new(),
        "Host: other.example\r\n".into(),
        format!("Host: {address}\r\nHost: {address}\r\n"),
    ] {
        let response = request_response(
            &server,
            format!("GET {path} HTTP/1.1\r\n{headers}\r\n").as_bytes(),
        );
        assert!(response.starts_with(b"HTTP/1.1 404"));
    }
    let response = request_response(
        &server,
        format!("GET {path} HTTP/1.1\r\nHost: {address}\r\n\r\nHost: other.example\r\n").as_bytes(),
    );
    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
}

#[test]
fn scoped_page_server_stops_without_retiring_other_origins() {
    let (first, path) = page_fixture(Duration::from_secs(1));
    let (second, _) = page_fixture(Duration::from_secs(1));
    let first_address = first.origin().strip_prefix("http://").unwrap().to_owned();
    drop(first);
    assert!(TcpStream::connect(first_address).is_err());
    assert!(second.is_running());
    let address = second.origin().strip_prefix("http://").unwrap();
    assert!(
        request_response(
            &second,
            format!("GET {path} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes()
        )
        .starts_with(b"HTTP/1.1 200 OK")
    );
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn product_font_css_uses_the_embedded_variable_face() {
    let css = get_font_css();
    assert!(css.contains("/font/"));
    assert!(css.contains("/GoogleSansFlex.ttf?v="));
    assert!(css.contains("font-weight: 1 1000"));
    assert!(css.contains("font-stretch: 25% 151%"));
    assert!(!css.contains("local('Segoe UI"));
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn windows_webviews_use_the_full_axis_web_font_bytes() {
    assert_eq!(PRODUCT_FONT_WOFF2, crate::assets::GOOGLE_SANS_FLEX_WEB);
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn child_font_url_is_loopback_and_session_scoped() {
    let raw = product_font_url().unwrap();
    let url = url::Url::parse(&raw).unwrap();
    let port = url.port().unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.port().is_some_and(|port| port > 0));
    let segments = url.path_segments().unwrap().collect::<Vec<_>>();
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0], "font");
    assert_eq!(segments[1].len(), 32);
    assert!(segments[1].bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(segments[2], "GoogleSansFlex.ttf");

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
    let request = format!(
        "GET {}?{} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        url.path(),
        url.query().unwrap()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let body_offset = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert_eq!(&response[body_offset..], PRODUCT_FONT_WOFF2);
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn idle_connection_does_not_block_a_page_request() {
    let base_url = get_server_url().unwrap();
    let url = url::Url::parse(&base_url).unwrap();
    let port = url.port().unwrap();
    let idle = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
    std::thread::sleep(Duration::from_millis(20));

    let page_url = url::Url::parse(&store_html_page("ready".into()).unwrap()).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", page_url.port().unwrap())).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        page_url.path()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    drop(idle);
    assert!(response.ends_with("ready"));
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn hosted_pages_are_idempotent_for_navigation_retries() {
    let page_url = url::Url::parse(&store_html_page("retry-safe".into()).unwrap()).unwrap();
    let port = page_url.port().unwrap();
    for _ in 0..2 {
        let mut stream = TcpStream::connect(("127.0.0.1", page_url.port().unwrap())).unwrap();
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
            page_url.path()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("retry-safe"));
    }
}

#[cfg(feature = "recorder-worker")]
#[test]
fn recorder_accepts_only_the_parent_font_route_contract() {
    let valid =
        "http://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc";
    let css = child_product_font_css(valid).unwrap();
    assert!(css.contains(valid));
    assert!(css.contains("format('woff2')"));
    for raw in [
        "https://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc",
        "http://localhost:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc",
        "http://127.0.0.1:43129/font/short/GoogleSansFlex.ttf?v=abc",
        "http://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/other.ttf?v=abc",
        "http://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf",
        "http://user@127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc",
    ] {
        assert!(child_product_font_css(raw).is_none(), "accepted {raw}");
    }
}
