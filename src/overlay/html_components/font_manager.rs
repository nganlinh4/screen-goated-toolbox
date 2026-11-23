//! Shared local-page host and product-font contract for WebView surfaces.
//!
//! Serves HTML pages and the full-axis Google Sans Flex web face from the same
//! local HTTP origin, avoiding CORS and private-network restrictions.

#[path = "font_manager/page_store.rs"]
mod page_store;
#[path = "font_manager/server.rs"]
mod server;
use crate::loopback_resource::Connection;
use page_store::PageStore;
use sha2::{Digest, Sha256};
#[cfg(not(feature = "recorder-worker"))]
use std::io::Write;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

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

static SERVER_URL: LazyLock<Mutex<Option<server::PageServer>>> = LazyLock::new(|| Mutex::new(None));
static PAGES: LazyLock<Mutex<PageStore>> = LazyLock::new(|| Mutex::new(PageStore::default()));

#[cfg(not(feature = "recorder-worker"))]
fn serve_product_font(
    stream: &mut Connection,
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
    _stream: &mut Connection,
    _method: &str,
    _path: &str,
    _cors_headers: &str,
) -> std::io::Result<bool> {
    Ok(false)
}

/// Get the server base URL, waiting if necessary
fn get_server_url() -> Option<String> {
    let mut server = SERVER_URL.lock().ok()?;
    if let Some(server) = server.as_ref().filter(|server| server.is_running()) {
        return Some(server.origin().to_owned());
    }
    *server = None;
    match server::PageServer::start() {
        Ok(started) => {
            let origin = started.origin().to_owned();
            *server = Some(started);
            Some(origin)
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
            src: url('{raw}') format('woff2');
        }}
    "#
    ))
}

#[cfg(test)]
#[path = "font_manager/tests.rs"]
mod tests;
