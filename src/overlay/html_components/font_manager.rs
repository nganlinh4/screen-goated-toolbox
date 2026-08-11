//! Shared local-page host and product-font contract for WebView surfaces.
//!
//! Serves HTML pages and the original Google Sans Flex variable face from the same
//! local HTTP origin, avoiding CORS and private-network restrictions.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Once, mpsc};
use std::time::Duration;
use wry::WebViewBuilder;

const LOCAL_PAGE_WORKERS: usize = 8;
const CONNECTION_IO_TIMEOUT: Duration = Duration::from_secs(2);

static START_SERVER_ONCE: Once = Once::new();
static PAGE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
#[cfg(not(feature = "recorder-worker"))]
static PRODUCT_FONT_TTF: &[u8] = crate::assets::GOOGLE_SANS_FLEX;
#[cfg(not(feature = "recorder-worker"))]
static FONT_ROUTE_TOKEN: LazyLock<Option<String>> = LazyLock::new(|| {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(
        bytes
            .iter()
            .fold(String::with_capacity(32), |mut token, byte| {
                use std::fmt::Write as _;
                let _ = write!(token, "{byte:02x}");
                token
            }),
    )
});

#[cfg(not(feature = "recorder-worker"))]
static SESSION_CACHE_BUSTER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
});

/// Server URL once started
static SERVER_URL: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

/// Pending HTML pages waiting to be served (page_id -> html)
static PENDING_PAGES: LazyLock<Mutex<HashMap<u64, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn start_server() {
    START_SERVER_ONCE.call_once(|| {
        std::thread::spawn(|| {
            let listener = match TcpListener::bind("127.0.0.1:0") {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind font server: {}", e);
                    return;
                }
            };

            let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
            let url = format!("http://127.0.0.1:{}", port);

            if let Ok(mut guard) = SERVER_URL.lock() {
                *guard = Some(url);
            }

            serve_connections(listener);
        });
    });
}

fn serve_connections(listener: TcpListener) {
    let (sender, receiver) = mpsc::channel::<TcpStream>();
    let receiver = Arc::new(Mutex::new(receiver));
    for worker_id in 0..LOCAL_PAGE_WORKERS {
        let receiver = Arc::clone(&receiver);
        let _ = std::thread::Builder::new()
            .name(format!("sgt-local-page-{worker_id}"))
            .spawn(move || connection_worker(receiver));
    }
    for stream in listener.incoming().flatten() {
        if sender.send(stream).is_err() {
            break;
        }
    }
}

fn connection_worker(receiver: Arc<Mutex<mpsc::Receiver<TcpStream>>>) {
    loop {
        let stream = receiver.lock().ok().and_then(|guard| guard.recv().ok());
        let Some(mut stream) = stream else {
            return;
        };
        let _ = stream.set_read_timeout(Some(CONNECTION_IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(CONNECTION_IO_TIMEOUT));
        let _ = handle_request(&mut stream);
    }
}

fn handle_request(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    let n = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..n]);

    // Parse the request line
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("GET");
    let path = parts.get(1).copied().unwrap_or("/");

    // CORS headers for all responses
    let cors_headers = "Access-Control-Allow-Origin: *\r\n\
                        Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n\
                        Access-Control-Allow-Headers: *\r\n\
                        Access-Control-Allow-Private-Network: true\r\n";

    // Handle OPTIONS preflight
    if method == "OPTIONS" {
        let response =
            format!("HTTP/1.1 204 No Content\r\n{cors_headers}Connection: close\r\n\r\n");
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // Route requests - strip query params for path matching
    let path_without_query = path.split('?').next().unwrap_or(path);

    if serve_product_font(stream, method, path_without_query, cors_headers)? {
        return Ok(());
    }

    if path_without_query.starts_with("/page/") {
        // Serve stored HTML page
        let id_str = path_without_query.strip_prefix("/page/").unwrap_or("0");
        let page_id: u64 = id_str.parse().unwrap_or(0);

        let html = PENDING_PAGES
            .lock()
            .ok()
            .and_then(|mut map| map.remove(&page_id))
            .unwrap_or_else(|| "<html><body>Page not found</body></html>".to_string());

        let html_bytes = html.as_bytes();
        let headers = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             {cors_headers}\
             Connection: close\r\n\r\n",
            html_bytes.len()
        );
        stream.write_all(headers.as_bytes())?;
        if method != "HEAD" {
            stream.write_all(html_bytes)?;
        }
    } else {
        // 404
        let body = b"Not Found";
        let headers = format!(
            "HTTP/1.1 404 Not Found\r\n\
             Content-Type: text/plain\r\n\
             Content-Length: {}\r\n\
             {cors_headers}\
             Connection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes())?;
        stream.write_all(body)?;
    }

    Ok(())
}

#[cfg(not(feature = "recorder-worker"))]
fn serve_product_font(
    stream: &mut std::net::TcpStream,
    method: &str,
    path: &str,
    cors_headers: &str,
) -> std::io::Result<bool> {
    let font_path = FONT_ROUTE_TOKEN
        .as_ref()
        .map(|token| format!("/font/{token}/GoogleSansFlex.ttf"));
    if font_path.as_deref() != Some(path) {
        return Ok(false);
    }
    let headers = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: font/ttf\r\n\
         Content-Length: {}\r\n\
         {cors_headers}\
         Cache-Control: max-age=3600\r\n\
         Connection: close\r\n\r\n",
        PRODUCT_FONT_TTF.len()
    );
    stream.write_all(headers.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(PRODUCT_FONT_TTF)?;
    }
    Ok(true)
}

#[cfg(feature = "recorder-worker")]
fn serve_product_font(
    _stream: &mut std::net::TcpStream,
    _method: &str,
    _path: &str,
    _cors_headers: &str,
) -> std::io::Result<bool> {
    Ok(false)
}

/// Get the server base URL, waiting if necessary
fn get_server_url() -> Option<String> {
    // Ensure server is started
    start_server();

    // Wait for URL to be available (up to 2 seconds)
    for _ in 0..40 {
        if let Ok(guard) = SERVER_URL.lock()
            && let Some(url) = guard.as_ref()
        {
            return Some(url.clone());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

/// Store HTML content and get a page URL to load it
pub fn store_html_page(html: String) -> Option<String> {
    let base_url = get_server_url()?;
    let page_id = PAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    if let Ok(mut map) = PENDING_PAGES.lock() {
        map.insert(page_id, html);
    }

    Some(format!("{}/page/{}", base_url, page_id))
}

/// Configure WebViewBuilder (no-op, URL loading handles everything)
pub fn configure_webview(builder: WebViewBuilder) -> WebViewBuilder {
    builder
}

/// Return the real product face for local WebView pages.
#[cfg(not(feature = "recorder-worker"))]
pub fn get_font_css() -> String {
    let font_url = product_font_url().unwrap_or_else(|| "about:blank".to_string());
    format!(
        r#"
        @font-face {{
            font-family: 'Google Sans Flex';
            font-style: normal;
            font-weight: 1 1000;
            font-stretch: 25% 151%;
            font-display: swap;
            src: url('{font_url}') format('truetype');
        }}
    "#
    )
}

/// Return a session-scoped loopback URL that child WebViews can use without
/// carrying another copy of the product font.
#[cfg(not(feature = "recorder-worker"))]
pub fn product_font_url() -> Option<String> {
    let base_url = get_server_url()?;
    let token = FONT_ROUTE_TOKEN.as_ref()?;
    Some(format!(
        "{base_url}/font/{token}/GoogleSansFlex.ttf?v={}",
        SESSION_CACHE_BUSTER.as_str()
    ))
}

#[cfg(feature = "recorder-worker")]
pub fn get_font_css() -> String {
    let Some(css) = std::env::var("SGT_PRODUCT_FONT_URL")
        .ok()
        .and_then(|raw| child_product_font_css(&raw))
    else {
        eprintln!("Recorder product-font URL is unavailable or invalid");
        return String::new();
    };
    css
}

#[cfg(feature = "recorder-worker")]
fn child_product_font_css(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none_or(|port| port == 0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let segments = url.path_segments()?.collect::<Vec<_>>();
    if segments.len() != 3
        || segments[0] != "font"
        || segments[1].len() != 32
        || !segments[1].bytes().all(|byte| byte.is_ascii_hexdigit())
        || segments[2] != "GoogleSansFlex.ttf"
        || url.query_pairs().count() != 1
        || !url
            .query_pairs()
            .any(|(key, value)| key == "v" && !value.is_empty())
    {
        return None;
    }
    Some(format!(
        r#"
        @font-face {{
            font-family: 'Google Sans Flex';
            font-style: normal;
            font-weight: 1 1000;
            font-stretch: 25% 151%;
            font-display: swap;
            src: url('{raw}') format('truetype');
        }}
    "#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn windows_webviews_use_the_original_product_font_bytes() {
        assert_eq!(PRODUCT_FONT_TTF, crate::assets::GOOGLE_SANS_FLEX);
    }

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn child_font_url_is_loopback_and_session_scoped() {
        let raw = product_font_url().unwrap();
        let url = url::Url::parse(&raw).unwrap();
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
            "GET {}?{} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
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
        assert_eq!(&response[body_offset..], PRODUCT_FONT_TTF);
    }

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn idle_connection_does_not_block_a_page_request() {
        let base_url = get_server_url().unwrap();
        let url = url::Url::parse(&base_url).unwrap();
        let idle = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
        std::thread::sleep(Duration::from_millis(20));

        let page_url = url::Url::parse(&store_html_page("ready".into()).unwrap()).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", page_url.port().unwrap())).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            page_url.path()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        drop(idle);
        assert!(response.ends_with("ready"));
    }

    #[cfg(feature = "recorder-worker")]
    #[test]
    fn recorder_accepts_only_the_parent_font_route_contract() {
        let valid =
            "http://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc";
        assert!(child_product_font_css(valid).unwrap().contains(valid));
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
}
