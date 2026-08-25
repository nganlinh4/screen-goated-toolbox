//! Shared local-page host and product-font contract for WebView surfaces.
//!
//! Serves HTML pages and the full-axis Google Sans Flex web face from the same
//! local HTTP origin, avoiding CORS and private-network restrictions.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};
use wry::WebViewBuilder;

const LOCAL_PAGE_WORKERS: usize = 8;
const LOCAL_PAGE_QUEUE: usize = 64;
const CONNECTION_IO_TIMEOUT: Duration = Duration::from_secs(2);
const PAGE_MAX_COUNT: usize = 128;
const PAGE_MAX_BYTES: usize = 32 * 1024 * 1024;
const PAGE_MAX_IDLE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[cfg(not(feature = "recorder-worker"))]
static PRODUCT_FONT_WOFF2: &[u8] = crate::assets::GOOGLE_SANS_FLEX_WEB;
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

static SERVER_URL: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static PAGES: LazyLock<Mutex<PageStore>> = LazyLock::new(|| Mutex::new(PageStore::default()));

struct PageEntry {
    html: Arc<str>,
    last_access: Instant,
}

#[derive(Default)]
struct PageStore {
    pages: HashMap<String, PageEntry>,
    total_bytes: usize,
}

impl PageStore {
    fn insert(&mut self, id: String, html: String, now: Instant) -> bool {
        self.prune_expired(now);
        let bytes = html.len();
        if bytes > PAGE_MAX_BYTES {
            return false;
        }
        if let Some(existing) = self.pages.get_mut(&id) {
            existing.last_access = now;
            return true;
        }
        while self.pages.len() >= PAGE_MAX_COUNT
            || self.total_bytes.saturating_add(bytes) > PAGE_MAX_BYTES
        {
            if !self.remove_oldest() {
                return false;
            }
        }
        self.total_bytes += bytes;
        self.pages.insert(
            id,
            PageEntry {
                html: Arc::from(html),
                last_access: now,
            },
        );
        true
    }

    fn get(&mut self, id: &str, now: Instant) -> Option<Arc<str>> {
        self.prune_expired(now);
        let entry = self.pages.get_mut(id)?;
        entry.last_access = now;
        Some(Arc::clone(&entry.html))
    }

    fn prune_expired(&mut self, now: Instant) {
        let expired = self
            .pages
            .iter()
            .filter_map(|(id, entry)| {
                (now.saturating_duration_since(entry.last_access) > PAGE_MAX_IDLE)
                    .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in expired {
            self.remove(&id);
        }
    }

    fn remove_oldest(&mut self) -> bool {
        let Some(id) = self
            .pages
            .iter()
            .min_by_key(|(_, entry)| entry.last_access)
            .map(|(id, _)| id.clone())
        else {
            return false;
        };
        self.remove(&id);
        true
    }

    fn remove(&mut self, id: &str) {
        if let Some(entry) = self.pages.remove(id) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.html.len());
        }
    }
}

fn start_server() -> Result<String, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let url = format!("http://127.0.0.1:{port}");
    std::thread::Builder::new()
        .name("sgt-local-page-server".to_string())
        .spawn(move || serve_connections(listener))
        .map_err(|error| error.to_string())?;
    Ok(url)
}

fn serve_connections(listener: TcpListener) {
    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(LOCAL_PAGE_QUEUE);
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
        let html = PAGES
            .lock()
            .ok()
            .and_then(|mut pages| pages.get(id_str, Instant::now()));
        let Some(html) = html else {
            let body = b"Page not found";
            let headers = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{cors_headers}Connection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes())?;
            if method != "HEAD" {
                stream.write_all(body)?;
            }
            return Ok(());
        };

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
         Content-Type: font/woff2\r\n\
         Content-Length: {}\r\n\
         {cors_headers}\
         Cache-Control: max-age=3600\r\n\
         Connection: close\r\n\r\n",
        PRODUCT_FONT_WOFF2.len()
    );
    stream.write_all(headers.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(PRODUCT_FONT_WOFF2)?;
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
    let mut server = SERVER_URL.lock().ok()?;
    if let Some(url) = server.as_ref() {
        return Some(url.clone());
    }
    match start_server() {
        Ok(url) => {
            *server = Some(url.clone());
            Some(url)
        }
        Err(error) => {
            eprintln!("Local WebView page server unavailable: {error}");
            None
        }
    }
}

/// Store HTML content and get a page URL to load it
pub fn store_html_page(html: String) -> Option<String> {
    let base_url = get_server_url()?;
    let page_id = format!("{:x}", Sha256::digest(html.as_bytes()));
    let inserted = PAGES
        .lock()
        .is_ok_and(|mut pages| pages.insert(page_id.clone(), html, Instant::now()));
    inserted.then(|| format!("{base_url}/page/{page_id}"))
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
            src: url('{font_url}') format('woff2');
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
    fn windows_webviews_use_the_full_axis_web_font_bytes() {
        assert_eq!(PRODUCT_FONT_WOFF2, crate::assets::GOOGLE_SANS_FLEX_WEB);
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
        assert_eq!(&response[body_offset..], PRODUCT_FONT_WOFF2);
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

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn hosted_pages_are_idempotent_for_navigation_retries() {
        let page_url = url::Url::parse(&store_html_page("retry-safe".into()).unwrap()).unwrap();
        for _ in 0..2 {
            let mut stream = TcpStream::connect(("127.0.0.1", page_url.port().unwrap())).unwrap();
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                page_url.path()
            );
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200 OK"));
            assert!(response.ends_with("retry-safe"));
        }
    }

    #[test]
    fn page_store_has_a_hard_entry_bound() {
        let mut store = PageStore::default();
        let now = Instant::now();
        for index in 0..PAGE_MAX_COUNT * 2 {
            assert!(store.insert(index.to_string(), "page".to_string(), now));
        }
        assert_eq!(store.pages.len(), PAGE_MAX_COUNT);
        assert!(store.total_bytes <= PAGE_MAX_BYTES);
    }

    #[cfg(feature = "recorder-worker")]
    #[test]
    fn recorder_accepts_only_the_parent_font_route_contract() {
        let valid =
            "http://127.0.0.1:43129/font/0123456789abcdef0123456789abcdef/GoogleSansFlex.ttf?v=abc";
        let css = child_product_font_css(valid).unwrap();
        assert!(css.contains(valid));
        assert!(css.contains("format('truetype')"));
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
