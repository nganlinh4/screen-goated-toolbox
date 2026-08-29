use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};
use wry::http::{Request, Response};

const MAX_TOKENS: usize = 128;
const MAX_DIRECT_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_DIRECT_IMAGE_DIMENSION: u32 = 32_768;
const MAX_DIRECT_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_DIRECT_SVG_BYTES: u64 = 12 * 1024 * 1024;

#[derive(Clone, Hash, PartialEq, Eq)]
enum AssetKind {
    ImagePreview { max_edge: u32 },
    SourceImage,
    StaticSvg,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct AssetKey {
    path: PathBuf,
    kind: AssetKind,
    size: u64,
    modified_ns: u128,
}

#[derive(Default)]
struct PreviewProtocolState {
    by_token: HashMap<String, AssetKey>,
    by_key: HashMap<AssetKey, String>,
    order: VecDeque<String>,
}

static STATE: LazyLock<Mutex<PreviewProtocolState>> =
    LazyLock::new(|| Mutex::new(PreviewProtocolState::default()));
static DECODE_LANE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn issue_from_args(args: &Value) -> Result<Value, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path is required".to_string())?;
    let max_edge = args
        .get("maxEdge")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    issue(path, max_edge)
}

pub(super) fn issue(path: &str, max_edge: Option<u32>) -> Result<Value, String> {
    issue_asset(
        path,
        AssetKind::ImagePreview {
            max_edge: max_edge.unwrap_or(1_600).clamp(64, 2_048),
        },
    )
}

pub(super) fn issue_source_image_from_args(args: &Value) -> Result<Value, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path is required".to_string())?;
    let canonical = std::fs::canonicalize(path).map_err(|_| "Image is unavailable.".to_string())?;
    validate_direct_image(&canonical)?;
    issue_asset_path(canonical, AssetKind::SourceImage)
}

pub(super) fn issue_static_svg(path: &str) -> Result<Value, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|_| "Vector result is unavailable.".to_string())?;
    let metadata = std::fs::symlink_metadata(&canonical)
        .map_err(|_| "Vector result is unavailable.".to_string())?;
    let is_svg = canonical
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"));
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_DIRECT_SVG_BYTES
        || !is_svg
    {
        return Err("Vector result is unavailable.".to_string());
    }
    issue_asset_path(canonical, AssetKind::StaticSvg)
}

fn issue_asset(path: &str, kind: AssetKind) -> Result<Value, String> {
    let path = std::fs::canonicalize(path).map_err(|_| "Image is unavailable.".to_string())?;
    issue_asset_path(path, kind)
}

fn issue_asset_path(path: PathBuf, kind: AssetKind) -> Result<Value, String> {
    let metadata = std::fs::metadata(&path).map_err(|_| "Image is unavailable.".to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("Image is unavailable.".to_string());
    }
    let key = AssetKey {
        path,
        kind,
        size: metadata.len(),
        modified_ns: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_nanos()),
    };
    let mut state = STATE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some(token) = state.by_key.get(&key) {
        return Ok(json!({ "url": asset_url(token, &key.kind) }));
    }
    let token = format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        key.modified_ns,
        TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let url = asset_url(&token, &key.kind);
    state.by_token.insert(token.clone(), key.clone());
    state.by_key.insert(key, token.clone());
    state.order.push_back(token.clone());
    while state.order.len() > MAX_TOKENS {
        let Some(expired) = state.order.pop_front() else {
            break;
        };
        if let Some(expired_key) = state.by_token.remove(&expired) {
            state.by_key.remove(&expired_key);
        }
    }
    Ok(json!({ "url": url }))
}

fn asset_url(token: &str, kind: &AssetKind) -> String {
    let origin = if cfg!(windows) {
        "http://sgtcreation.localhost"
    } else {
        "sgtcreation://localhost"
    };
    let route = match kind {
        AssetKind::ImagePreview { .. } => "image",
        AssetKind::SourceImage => "source",
        AssetKind::StaticSvg => "svg",
    };
    format!("{origin}/{route}/{token}")
}

fn validate_direct_image(path: &Path) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "Image is unavailable.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_DIRECT_IMAGE_BYTES
        || direct_image_mime(path).is_none()
    {
        return Err("Image is unavailable.".to_string());
    }
    let dimensions = image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|_| "Image is unavailable.".to_string())?
        .into_dimensions()
        .map_err(|_| "Image is unavailable.".to_string())?;
    let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 > MAX_DIRECT_IMAGE_DIMENSION
        || dimensions.1 > MAX_DIRECT_IMAGE_DIMENSION
        || pixels > MAX_DIRECT_IMAGE_PIXELS
    {
        return Err("Image is unavailable.".to_string());
    }
    Ok(())
}

fn direct_image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn response(
    status: u16,
    mime: &'static str,
    body: Cow<'static, [u8]>,
    cache: &'static str,
) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cross-Origin-Resource-Policy", "cross-origin")
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", cache)
        .body(body)
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"Internal Error")))
}

pub(super) fn handle(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    if request.headers().contains_key("range") {
        return response(
            416,
            "text/plain",
            Cow::Borrowed(b"Range requests are not supported."),
            "no-store",
        );
    }
    let path = request.uri().path();
    let (token, route) = if let Some(token) = path.strip_prefix("/image/") {
        (token, "image")
    } else if let Some(token) = path.strip_prefix("/source/") {
        (token, "source")
    } else if let Some(token) = path.strip_prefix("/svg/") {
        (token, "svg")
    } else {
        return response(404, "text/plain", Cow::Borrowed(b"Not Found"), "no-store");
    };
    if token.is_empty() || token.contains('/') {
        return response(404, "text/plain", Cow::Borrowed(b"Not Found"), "no-store");
    }
    let key = STATE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .by_token
        .get(token)
        .cloned();
    let Some(key) = key else {
        return response(404, "text/plain", Cow::Borrowed(b"Not Found"), "no-store");
    };
    let current = std::fs::metadata(&key.path).ok();
    let unchanged = current.as_ref().is_some_and(|metadata| {
        metadata.is_file()
            && metadata.len() == key.size
            && metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_nanos())
                == key.modified_ns
    });
    if !unchanged {
        return response(410, "text/plain", Cow::Borrowed(b"Gone"), "no-store");
    }
    match (&key.kind, route) {
        (AssetKind::SourceImage, "source") => {
            return match std::fs::read(&key.path) {
                Ok(bytes) => response(
                    200,
                    direct_image_mime(&key.path).unwrap_or("application/octet-stream"),
                    Cow::Owned(bytes),
                    "no-store",
                ),
                Err(_) => response(
                    422,
                    "text/plain",
                    Cow::Borrowed(b"Preview unavailable"),
                    "no-store",
                ),
            };
        }
        (AssetKind::StaticSvg, "svg") => {
            return match std::fs::read(&key.path) {
                Ok(bytes) => Response::builder()
                    .status(200)
                    .header("Content-Type", "image/svg+xml")
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Cross-Origin-Resource-Policy", "cross-origin")
                    .header("X-Content-Type-Options", "nosniff")
                    .header(
                        "Content-Security-Policy",
                        "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox",
                    )
                    .header("Cache-Control", "no-store")
                    .body(Cow::Owned(bytes))
                    .unwrap_or_else(|_| Response::new(Cow::<[u8]>::Borrowed(b"Internal Error"))),
                Err(_) => response(
                    422,
                    "text/plain",
                    Cow::Borrowed(b"Preview unavailable"),
                    "no-store",
                ),
            };
        }
        (AssetKind::ImagePreview { .. }, "image") => {}
        _ => return response(404, "text/plain", Cow::Borrowed(b"Not Found"), "no-store"),
    }
    let _decode_permit = DECODE_LANE
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let AssetKind::ImagePreview { max_edge } = key.kind else {
        unreachable!("route and asset kind were checked above");
    };
    match super::creation_preview::encode_image_preview(&key.path, Some(max_edge)) {
        Ok(preview) => response(
            200,
            preview.mime,
            Cow::Owned(preview.encoded),
            "public, max-age=31536000, immutable",
        ),
        Err(_) => response(
            422,
            "text/plain",
            Cow::Borrowed(b"Preview unavailable"),
            "no-store",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{handle, issue, issue_source_image_from_args, issue_static_svg};
    use image::{ImageBuffer, Rgba};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};
    use wry::http::Request;

    #[test]
    fn issued_preview_streams_bounded_image_bytes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sgt-preview-protocol-{}-{unique}.png",
            std::process::id()
        ));
        ImageBuffer::from_pixel(320, 160, Rgba([12_u8, 34, 56, 200]))
            .save(&path)
            .unwrap();

        let first = issue(path.to_str().unwrap(), Some(80)).unwrap();
        let second = issue(path.to_str().unwrap(), Some(80)).unwrap();
        assert_eq!(first, second);
        let url = first["url"].as_str().unwrap();
        let request = Request::builder().uri(url).body(Vec::new()).unwrap();
        let response = handle(request);
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["Content-Type"], "image/png");
        let decoded = image::load_from_memory(response.body()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (80, 40));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn selected_image_streams_original_bytes_without_a_persistent_cache() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sgt-source-protocol-{}-{unique}.png",
            std::process::id()
        ));
        ImageBuffer::from_pixel(32, 16, Rgba([12_u8, 34, 56, 200]))
            .save(&path)
            .unwrap();
        let original = std::fs::read(&path).unwrap();
        let issued =
            issue_source_image_from_args(&json!({ "path": path.to_str().unwrap() })).unwrap();
        let request = Request::builder()
            .uri(issued["url"].as_str().unwrap())
            .body(Vec::new())
            .unwrap();
        let streamed = handle(request);
        assert_eq!(streamed.status(), 200);
        assert_eq!(streamed.headers()["Cache-Control"], "no-store");
        assert_eq!(streamed.body().as_ref(), original);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn static_svg_stream_is_inert_and_not_cached() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sgt-static-svg-{}-{unique}.svg",
            std::process::id()
        ));
        let original = br#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#;
        std::fs::write(&path, original).unwrap();
        let issued = issue_static_svg(path.to_str().unwrap()).unwrap();
        let request = Request::builder()
            .uri(issued["url"].as_str().unwrap())
            .body(Vec::new())
            .unwrap();
        let streamed = handle(request);
        assert_eq!(streamed.status(), 200);
        assert_eq!(streamed.headers()["Cache-Control"], "no-store");
        assert_eq!(streamed.headers()["Content-Type"], "image/svg+xml");
        assert!(streamed.headers().contains_key("Content-Security-Policy"));
        assert_eq!(streamed.body().as_ref(), original);
        std::fs::remove_file(path).unwrap();
    }
}
