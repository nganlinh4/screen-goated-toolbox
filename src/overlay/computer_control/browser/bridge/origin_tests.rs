use super::*;
use tungstenite::http::HeaderValue;

#[test]
fn present_origin_must_be_one_exact_extension_origin() {
    let id = "abcdefghijklmnopabcdefghijklmnop";
    let mut headers = HeaderMap::new();
    assert!(allowed(&headers, Some(id), false));
    assert!(matches_claim(&headers, Some(id)));
    for value in [
        "null",
        "https://example.test",
        "http://127.0.0.1:47800",
        "http://abcdefghijklmnopabcdefghijklmnop",
        "ws://abcdefghijklmnopabcdefghijklmnop",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop:80",
        "chrome-extension://user@abcdefghijklmnopabcdefghijklmnop",
        "file://",
        "chrome-extension://abc",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/path",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop?x=1",
    ] {
        headers.insert("origin", HeaderValue::from_str(value).unwrap());
        assert!(!allowed(&headers, None, true), "{value}");
    }
    headers.insert(
        "origin",
        HeaderValue::from_static("chrome-extension://abcdefghijklmnopabcdefghijklmnop"),
    );
    assert!(allowed(&headers, Some(id), false));
    assert!(matches_claim(&headers, Some(id)));
    assert!(!matches_claim(
        &headers,
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    ));
    assert!(!allowed(
        &headers,
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        false
    ));
    assert!(allowed(
        &headers,
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        true
    ));
    headers.append("origin", HeaderValue::from_static("null"));
    assert!(!allowed(&headers, None, true));
    headers.clear();
    headers.insert("origin", HeaderValue::from_bytes(&[0xff]).unwrap());
    assert!(!allowed(&headers, None, true));
}
