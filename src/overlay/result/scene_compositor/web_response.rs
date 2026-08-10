use std::borrow::Cow;
use wry::http::Response;

pub(super) fn compositor_response(
    status: u16,
    mime: &'static str,
    body: Cow<'static, [u8]>,
    cache_control: &'static str,
) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", cache_control)
        .body(body)
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"Internal Error")))
}
