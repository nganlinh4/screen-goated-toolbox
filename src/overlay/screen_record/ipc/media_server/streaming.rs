use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use std::thread;

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
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg",
        "aiff" | "aif" => "audio/aiff",
        "wma" => "audio/x-ms-wma",
        "alac" => "audio/mp4",
        "mka" => "audio/x-matroska",
        "gif" => "image/gif",
        _ => "video/mp4",
    }
    .to_string()
}

pub(super) fn spawn_media_file_response(
    request: tiny_http::Request,
    media_path_str: String,
    range_header_str: Option<String>,
) {
    thread::spawn(move || {
        let file_size = match std::fs::metadata(&media_path_str) {
            Ok(m) => m.len(),
            Err(_) => {
                let mut res = Response::from_string("File error").with_status_code(500);
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

        let mut start: u64 = 0;
        let mut end: u64 = file_size.saturating_sub(1);
        let mut is_partial = false;

        if let Some(r) = range_header_str.as_deref() {
            let parts: Vec<&str> = r.split('-').collect();
            if parts.len() == 2 {
                let start_part = parts[0].trim();
                let end_part = parts[1].trim();
                if !start_part.is_empty() {
                    if let Ok(s) = start_part.parse::<u64>() {
                        start = s.min(file_size.saturating_sub(1));
                        if !end_part.is_empty()
                            && let Ok(e) = end_part.parse::<u64>()
                        {
                            end = e.min(file_size.saturating_sub(1));
                        }
                        is_partial = true;
                    }
                } else if !end_part.is_empty()
                    && let Ok(suffix_len) = end_part.parse::<u64>()
                {
                    let clamped_suffix = suffix_len.min(file_size);
                    start = file_size.saturating_sub(clamped_suffix);
                    end = file_size.saturating_sub(1);
                    is_partial = true;
                }
            }
        }

        if start > end || start >= file_size {
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

        end = end.min(file_size.saturating_sub(1));
        let content_len = end.saturating_sub(start).saturating_add(1);

        if let Ok(mut f) = File::open(&media_path_str) {
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
                        content_type_for_path(&media_path_str).as_bytes(),
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
    });
}
