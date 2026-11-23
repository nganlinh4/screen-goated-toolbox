use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use tiny_http::{Response, StatusCode};

fn cors_header() -> tiny_http::Header {
    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
}

fn content_type_for_path(media_path_str: &str) -> String {
    match Path::new(media_path_str)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg",
        "aiff" | "aif" => "audio/aiff",
        "alac" => "audio/mp4",
        "mka" => "audio/x-matroska",
        "gif" => "image/gif",
        _ => "video/mp4",
    }
    .to_string()
}

#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    Whole,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

/// Parses the value after `bytes=` against a non-empty file. Only the end of a
/// range is clamped; a start at or past EOF is unsatisfiable (RFC 9110 14.1.2).
fn parse_byte_range(spec: Option<&str>, file_size: u64) -> ByteRange {
    let Some((start_part, end_part)) = spec.and_then(|spec| spec.split_once('-')) else {
        return ByteRange::Whole;
    };
    let (start_part, end_part) = (start_part.trim(), end_part.trim());
    let last = file_size.saturating_sub(1);
    if start_part.is_empty() {
        return match end_part.parse::<u64>() {
            Ok(0) => ByteRange::Unsatisfiable,
            Ok(suffix) => ByteRange::Partial {
                start: file_size.saturating_sub(suffix.min(file_size)),
                end: last,
            },
            Err(_) => ByteRange::Whole,
        };
    }
    let Ok(start) = start_part.parse::<u64>() else {
        return ByteRange::Whole;
    };
    let end = if end_part.is_empty() {
        last
    } else {
        match end_part.parse::<u64>() {
            Ok(end) => end.min(last),
            Err(_) => return ByteRange::Whole,
        }
    };
    if start >= file_size || start > end {
        return ByteRange::Unsatisfiable;
    }
    ByteRange::Partial { start, end }
}

pub(super) fn respond_media_file(
    request: tiny_http::Request,
    media_path_str: &str,
    range_header_str: Option<&str>,
) {
    let file_size = match std::fs::metadata(media_path_str) {
        Ok(m) if m.is_file() => m.len(),
        _ => {
            let mut res = Response::from_string("File not found").with_status_code(404);
            res.add_header(cors_header());
            let _ = request.respond(res);
            return;
        }
    };

    if file_size == 0 {
        let mut res = Response::empty(200);
        res.add_header(cors_header());
        let _ = request.respond(res);
        return;
    }

    let (start, end, is_partial) = match parse_byte_range(range_header_str, file_size) {
        ByteRange::Whole => (0, file_size - 1, false),
        ByteRange::Partial { start, end } => (start, end, true),
        ByteRange::Unsatisfiable => {
            let mut res =
                Response::from_string("Requested range not satisfiable").with_status_code(416);
            res.add_header(cors_header());
            res.add_header(
                tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
            );
            res.add_header(
                tiny_http::Header::from_bytes(
                    &b"Content-Range"[..],
                    format!("bytes */{}", file_size).as_bytes(),
                )
                .unwrap(),
            );
            let _ = request.respond(res);
            return;
        }
    };
    let content_len = end - start + 1;

    if let Ok(mut f) = File::open(media_path_str) {
        let _ = f.seek(std::io::SeekFrom::Start(start));
        let mut res = Response::new(
            if is_partial {
                StatusCode(206)
            } else {
                StatusCode(200)
            },
            vec![
                tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    content_type_for_path(media_path_str).as_bytes(),
                )
                .unwrap(),
                cors_header(),
                tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                tiny_http::Header::from_bytes(
                    &b"Content-Length"[..],
                    content_len.to_string().as_bytes(),
                )
                .unwrap(),
                // Tell the client to close the connection after each response
                // so keepalive connections don't accumulate while old streams
                // are in-flight (which would starve new seek requests).
                tiny_http::Header::from_bytes(&b"Connection"[..], &b"close"[..]).unwrap(),
            ],
            Box::new(f.take(content_len)) as Box<dyn Read + Send>,
            Some(content_len as usize),
            None,
        );
        if is_partial {
            res.add_header(
                tiny_http::Header::from_bytes(
                    &b"Content-Range"[..],
                    format!("bytes {}-{}/{}", start, end, file_size).as_bytes(),
                )
                .unwrap(),
            );
        }
        let _ = request.respond(res);
    } else {
        let mut res = Response::from_string("File not found").with_status_code(404);
        res.add_header(cors_header());
        let _ = request.respond(res);
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteRange, parse_byte_range};

    #[test]
    fn byte_ranges_follow_rfc_semantics() {
        let size = 4_000_000;
        assert_eq!(parse_byte_range(None, size), ByteRange::Whole);
        assert_eq!(
            parse_byte_range(Some("100-"), size),
            ByteRange::Partial {
                start: 100,
                end: size - 1
            }
        );
        assert_eq!(
            parse_byte_range(Some("100-9999999"), size),
            ByteRange::Partial {
                start: 100,
                end: size - 1
            }
        );
        assert_eq!(
            parse_byte_range(Some("-500"), size),
            ByteRange::Partial {
                start: size - 500,
                end: size - 1
            }
        );
        assert_eq!(
            parse_byte_range(Some("-99999999"), size),
            ByteRange::Partial {
                start: 0,
                end: size - 1
            }
        );
        assert_eq!(
            parse_byte_range(Some("500-100"), size),
            ByteRange::Unsatisfiable
        );
        assert_eq!(parse_byte_range(Some("-0"), size), ByteRange::Unsatisfiable);
    }

    #[test]
    fn range_starting_past_the_end_is_unsatisfiable() {
        assert_eq!(
            parse_byte_range(Some("5000000-"), 4_000_000),
            ByteRange::Unsatisfiable
        );
        assert_eq!(
            parse_byte_range(Some("4000000-"), 4_000_000),
            ByteRange::Unsatisfiable
        );
    }
}
