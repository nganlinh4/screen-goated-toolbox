// --- GLOBAL MEDIA SERVER ---
// HTTP media server for streaming recorded video/audio files with
// range-request support, plus POST endpoints for staging atlas data.

use super::super::SERVER_PORT;
use super::super::native_export;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use std::thread;
use tiny_http::{Response, Server, StatusCode};

pub fn start_global_media_server() -> Result<u16, String> {
    let mut port = 8000;
    let server = loop {
        match Server::http(format!("127.0.0.1:{}", port)) {
            Ok(s) => break s,
            Err(_) => {
                port += 1;
                if port > 9000 {
                    return Err("No port available".to_string());
                }
            }
        }
    };

    let actual_port = port;
    SERVER_PORT.store(actual_port, std::sync::atomic::Ordering::SeqCst);

    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            if request.method() == &tiny_http::Method::Options {
                let mut res = Response::empty(204);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(
                        &b"Access-Control-Allow-Methods"[..],
                        &b"GET, POST, OPTIONS"[..],
                    )
                    .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(
                        &b"Access-Control-Allow-Headers"[..],
                        &b"Range, Content-Type"[..],
                    )
                    .unwrap(),
                );
                let _ = request.respond(res);
                continue;
            }

            // POST /stage-atlas?w=N&h=N — binary RGBA body staged directly for export.
            // Eliminates PNG encode (JS) + base64 + PNG decode (Rust) round-trip.
            if request.method() == &tiny_http::Method::Post
                && request.url().starts_with("/stage-atlas")
            {
                let cors =
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap();
                let url = request.url().to_string();
                let qs = url.split_once('?').map(|(_, q)| q).unwrap_or("");
                let find_param = |name: &str| -> Option<u32> {
                    qs.split('&')
                        .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
                        .and_then(|v| v.parse().ok())
                };
                let w: u32 = find_param("w").unwrap_or(1);
                let h: u32 = find_param("h").unwrap_or(1);
                let mut body = Vec::new();
                if request.as_reader().read_to_end(&mut body).is_ok() && !body.is_empty() {
                    let find_str_param = |name: &str| -> Option<String> {
                        let raw = qs
                            .split('&')
                            .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))?;
                        Some(urlencoding::decode(raw).unwrap_or_default().into_owned())
                    };
                    let session_job = find_str_param("session").zip(find_str_param("job"));

                    // Body is PNG binary — decode to RGBA (skips base64 layer).
                    let expected_rgba = (w as usize) * (h as usize) * 4;
                    let rgba = if body.len() == expected_rgba {
                        // Raw RGBA — use directly
                        body
                    } else {
                        // PNG binary — decode
                        match image::load_from_memory(&body) {
                            Ok(img) => {
                                let rgba8 = img.to_rgba8();
                                let actual_w = rgba8.width();
                                let actual_h = rgba8.height();
                                if actual_w != w || actual_h != h {
                                    eprintln!(
                                        "[stage-atlas] Decoded {}x{} but expected {}x{}",
                                        actual_w, actual_h, w, h
                                    );
                                }
                                rgba8.into_raw()
                            }
                            Err(e) => {
                                let msg = format!("Atlas PNG decode failed: {e}");
                                eprintln!("[stage-atlas] {msg}");
                                let mut res = Response::from_string(msg).with_status_code(400);
                                res.add_header(cors);
                                let _ = request.respond(res);
                                continue;
                            }
                        }
                    };

                    if let Some((sid, jid)) = session_job {
                        native_export::staging::set_atlas_for(&sid, &jid, rgba, w, h);
                    } else {
                        native_export::staging::set_atlas(rgba, w, h);
                    }
                    let mut res = Response::from_string(r#"{"ok":true}"#).with_status_code(200);
                    res.add_header(cors);
                    let _ = request.respond(res);
                } else {
                    let mut res = Response::from_string("Empty body").with_status_code(400);
                    res.add_header(cors);
                    let _ = request.respond(res);
                }
                continue;
            }

            // POST /write-temp — write binary body to recordings dir, return file path.
            // Used to restore rawVideoPath for old projects that only have a blob.
            if request.method() == &tiny_http::Method::Post
                && request.url().starts_with("/write-temp")
            {
                let cors =
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap();
                let recordings_dir = dirs::data_local_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("screen-goated-toolbox")
                    .join("recordings");
                let _ = std::fs::create_dir_all(&recordings_dir);
                let dest = recordings_dir.join(format!(
                    "restored_{}.mp4",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                ));
                let mut body = Vec::new();
                if request.as_reader().read_to_end(&mut body).is_ok()
                    && !body.is_empty()
                    && std::fs::write(&dest, &body).is_ok()
                {
                    let json =
                        format!("{{\"path\":{}}}", serde_json::json!(dest.to_string_lossy()));
                    let mut res = Response::from_string(json).with_status_code(200);
                    res.add_header(cors);
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    );
                    let _ = request.respond(res);
                } else {
                    let mut res = Response::from_string("Write failed").with_status_code(500);
                    res.add_header(cors);
                    let _ = request.respond(res);
                }
                continue;
            }

            let url = request.url();
            let media_path_str = if let Some(idx) = url.find("?path=") {
                let encoded = &url[idx + 6..];
                urlencoding::decode(encoded)
                    .unwrap_or(std::borrow::Cow::Borrowed(""))
                    .into_owned()
            } else {
                String::new()
            };
            if media_path_str.is_empty() || !Path::new(&media_path_str).exists() {
                let mut res = Response::from_string("File not found").with_status_code(404);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                let _ = request.respond(res);
                continue;
            }

            let content_type = match Path::new(&media_path_str)
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
                "gif" => "image/gif",
                _ => "video/mp4",
            }
            .to_string();

            // Extract the Range header value now (before moving `request` into the thread).
            let range_header_str: Option<String> = request
                .headers()
                .iter()
                .find(|h| h.field.to_string().eq_ignore_ascii_case("range"))
                .and_then(|h| h.value.as_str().strip_prefix("bytes="))
                .map(|s| s.to_owned());

            // Spawn a thread for the actual file I/O so that streaming a large range
            // never blocks the server loop from accepting the next (e.g. seek) request.
            thread::spawn(move || {
                let file_size = match std::fs::metadata(&media_path_str) {
                    Ok(m) => m.len(),
                    Err(_) => {
                        let mut res = Response::from_string("File error").with_status_code(500);
                        res.add_header(
                            tiny_http::Header::from_bytes(
                                &b"Access-Control-Allow-Origin"[..],
                                &b"*"[..],
                            )
                            .unwrap(),
                        );
                        let _ = request.respond(res);
                        return;
                    }
                };

                if file_size == 0 {
                    let mut res = Response::empty(200);
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Access-Control-Allow-Origin"[..],
                            &b"*"[..],
                        )
                        .unwrap(),
                    );
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
                    let mut res = Response::from_string("Requested range not satisfiable")
                        .with_status_code(416);
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Access-Control-Allow-Origin"[..],
                            &b"*"[..],
                        )
                        .unwrap(),
                    );
                    res.add_header(
                        tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..])
                            .unwrap(),
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
                                content_type.as_bytes(),
                            )
                            .unwrap(),
                            tiny_http::Header::from_bytes(
                                &b"Access-Control-Allow-Origin"[..],
                                &b"*"[..],
                            )
                            .unwrap(),
                            tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..])
                                .unwrap(),
                            tiny_http::Header::from_bytes(
                                &b"Content-Length"[..],
                                content_len.to_string().as_bytes(),
                            )
                            .unwrap(),
                            // Tell the client to close the connection after each response
                            // so keepalive connections don't accumulate while old streams
                            // are in-flight (which would starve new seek requests).
                            tiny_http::Header::from_bytes(&b"Connection"[..], &b"close"[..])
                                .unwrap(),
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
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Access-Control-Allow-Origin"[..],
                            &b"*"[..],
                        )
                        .unwrap(),
                    );
                    let _ = request.respond(res);
                }
            });
        }
    });

    Ok(actual_port)
}
