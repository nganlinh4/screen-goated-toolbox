use std::borrow::Cow;

const INDEX_HTML: &[u8] = include_bytes!("dist/index.html");
const ASSET_INDEX_JS: &[u8] = include_bytes!("dist/assets/index.js");
const ASSET_INDEX_CSS: &[u8] = include_bytes!("dist/assets/index.css");

pub(super) fn lookup_packaged_asset(path: &str) -> Option<(Cow<'static, [u8]>, &'static str)> {
    match path {
        "/" | "/index.html" => Some((Cow::Borrowed(INDEX_HTML), "text/html")),
        "/assets/index.js" => Some((Cow::Borrowed(ASSET_INDEX_JS), "application/javascript")),
        "/assets/index.css" => Some((Cow::Borrowed(ASSET_INDEX_CSS), "text/css")),
        _ => None,
    }
}

pub(super) fn wnd_http_response(
    status: u16,
    content_type: &str,
    body: Cow<'static, [u8]>,
) -> wry::http::Response<Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| {
            wry::http::Response::builder()
                .status(500)
                .body(Cow::Borrowed(b"Internal Error".as_slice()))
                .unwrap()
        })
}
