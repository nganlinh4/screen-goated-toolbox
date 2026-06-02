// --- GLOBAL MEDIA SERVER ---
// HTTP media server for streaming recorded video/audio files with
// range-request support, plus POST endpoints for staging atlas data.

mod audio_import;
mod import_normalize;
mod streaming;

use super::super::SERVER_PORT;
use super::super::native_export;

pub use self::audio_import::{
    create_audio_placeholder_video, import_audio_path_to_managed_media_file,
};
use self::audio_import::{managed_import_audio_path, normalized_audio_extension};

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;
use tiny_http::{Response, Server};

const NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE: u32 = 48_000;
const NORMALIZED_IMPORT_AUDIO_CHANNELS: u32 = 2;
const NORMALIZED_IMPORT_AUDIO_BITRATE_KBPS: u32 = 192;

pub(crate) fn recordings_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("screen-goated-toolbox")
        .join("recordings")
}

fn normalize_imported_video(
    input_path: &Path,
    output_path: &Path,
    trace_id: &str,
) -> Result<bool, String> {
    import_normalize::normalize_imported_video_mf(input_path, output_path, trace_id)
}

fn managed_import_path(recordings_dir: &Path, ts: u128, extension: &str) -> PathBuf {
    recordings_dir.join(format!("imported-{ts}.{extension}"))
}

pub(crate) fn write_managed_narration_wav(
    trace_id: &str,
    index: usize,
    wav_data: &[u8],
) -> Result<String, String> {
    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let safe_trace_id: String = trace_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let output_path = recordings_dir.join(format!("narration-{safe_trace_id}-{index}-{ts}.wav"));
    std::fs::write(&output_path, wav_data).map_err(|error| {
        format!(
            "Write narration WAV failed at '{}': {error}",
            output_path.display()
        )
    })?;
    Ok(output_path.to_string_lossy().to_string())
}

pub fn import_video_path_to_managed_media_file(
    source_path: &Path,
    trace_id: &str,
) -> Result<(String, bool), String> {
    if !source_path.exists() || !source_path.is_file() {
        return Err(format!("Video file not found: {}", source_path.display()));
    }

    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .unwrap_or("mp4");
    let output_path = managed_import_path(&recordings_dir, ts, extension);
    let started_at = Instant::now();

    std::fs::copy(source_path, &output_path).map_err(|error| {
        format!(
            "Copy imported video failed from '{}' to '{}': {error}",
            source_path.display(),
            output_path.display()
        )
    })?;
    let has_audio = import_normalize::probe_media_has_audio(&output_path)?;
    crate::log_info!(
        "[VideoImport:{}][Path] complete total {:.3}s file=\"{}\" output=\"{}\" has_audio={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        source_path.display(),
        output_path.display(),
        has_audio
    );

    Ok((output_path.to_string_lossy().to_string(), has_audio))
}

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
                let recordings_dir = recordings_dir();
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

            if request.method() == &tiny_http::Method::Post
                && request.url().starts_with("/import-video")
            {
                let cors =
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap();
                let recordings_dir = recordings_dir();
                let _ = std::fs::create_dir_all(&recordings_dir);
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let url = request.url().to_string();
                let file_name = url
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or("")
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("filename="))
                    .and_then(|value| urlencoding::decode(value).ok())
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|| "upload.mp4".to_string());
                let trace_id = url
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or("")
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("traceId="))
                    .and_then(|value| urlencoding::decode(value).ok())
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|| format!("video-import-{ts}"));
                let extension = Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .filter(|ext| !ext.is_empty())
                    .unwrap_or("mp4");
                let normalize_requested = url
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or("")
                    .split('&')
                    .any(|kv| kv == "normalize=1" || kv == "normalize=true");
                let input_path = recordings_dir.join(format!("import-source-{ts}.{extension}"));
                let output_path = managed_import_path(&recordings_dir, ts, extension);
                let mut body = Vec::new();
                let request_started_at = Instant::now();

                let result = (|| -> Result<(String, bool), String> {
                    request
                        .as_reader()
                        .read_to_end(&mut body)
                        .map_err(|e| format!("Read import body: {e}"))?;
                    if body.is_empty() {
                        return Err("Uploaded video is empty".to_string());
                    }
                    std::fs::write(&input_path, &body)
                        .map_err(|e| format!("Write imported video temp file: {e}"))?;
                    if normalize_requested {
                        let normalize_output_path =
                            recordings_dir.join(format!("imported-{ts}.mp4"));
                        let has_audio = normalize_imported_video(
                            &input_path,
                            &normalize_output_path,
                            &trace_id,
                        )?;
                        return Ok((
                            normalize_output_path.to_string_lossy().to_string(),
                            has_audio,
                        ));
                    }

                    std::fs::rename(&input_path, &output_path).or_else(|rename_error| {
                        std::fs::copy(&input_path, &output_path)
                            .map(|_| ())
                            .map_err(|copy_error| {
                                format!(
                                    "Persist imported video failed (rename: {rename_error}; copy: {copy_error})"
                                )
                            })
                    })?;
                    let has_audio = import_normalize::probe_media_has_audio(&output_path)?;
                    Ok((output_path.to_string_lossy().to_string(), has_audio))
                })();

                let _ = std::fs::remove_file(&input_path);

                match result {
                    Ok((path, has_audio)) => {
                        crate::log_info!(
                            "[VideoImport:{}][HTTP] complete total {:.3}s size_mb={:.2} file=\"{}\" output=\"{}\" has_audio={}",
                            trace_id,
                            request_started_at.elapsed().as_secs_f64(),
                            body.len() as f64 / (1024.0 * 1024.0),
                            file_name,
                            path,
                            has_audio
                        );
                        let json = format!(
                            "{{\"path\":{},\"hasAudio\":{}}}",
                            serde_json::json!(path),
                            if has_audio { "true" } else { "false" }
                        );
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
                    }
                    Err(error) => {
                        crate::log_info!(
                            "[VideoImport:{}][HTTP] failed after {:.3}s: {}",
                            trace_id,
                            request_started_at.elapsed().as_secs_f64(),
                            error
                        );
                        let _ = std::fs::remove_file(&output_path);
                        let mut res = Response::from_string(error).with_status_code(500);
                        res.add_header(cors);
                        let _ = request.respond(res);
                    }
                }
                continue;
            }

            if request.method() == &tiny_http::Method::Post
                && request.url().starts_with("/import-audio")
            {
                let cors =
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap();
                let recordings_dir = recordings_dir();
                let _ = std::fs::create_dir_all(&recordings_dir);
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let url = request.url().to_string();
                let file_name = url
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or("")
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("filename="))
                    .and_then(|value| urlencoding::decode(value).ok())
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|| "upload.mp3".to_string());
                let trace_id = url
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or("")
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("traceId="))
                    .and_then(|value| urlencoding::decode(value).ok())
                    .map(|value| value.into_owned())
                    .unwrap_or_else(|| format!("audio-import-{ts}"));
                let raw_ext = Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .filter(|ext| !ext.is_empty())
                    .unwrap_or("mp3");
                let extension = normalized_audio_extension(raw_ext);
                let output_path = managed_import_audio_path(&recordings_dir, ts, extension);
                let mut body = Vec::new();
                let request_started_at = Instant::now();

                let result = (|| -> Result<(String, f64), String> {
                    request
                        .as_reader()
                        .read_to_end(&mut body)
                        .map_err(|e| format!("Read import audio body: {e}"))?;
                    if body.is_empty() {
                        return Err("Uploaded audio is empty".to_string());
                    }
                    std::fs::write(&output_path, &body)
                        .map_err(|e| format!("Write imported audio file: {e}"))?;
                    let duration_sec =
                        import_normalize::probe_audio_duration_seconds(&output_path).unwrap_or(0.0);
                    Ok((output_path.to_string_lossy().to_string(), duration_sec))
                })();

                match result {
                    Ok((path, duration_sec)) => {
                        crate::log_info!(
                            "[AudioImport:{}][HTTP] complete total {:.3}s size_mb={:.2} file=\"{}\" output=\"{}\" duration={:.3}s",
                            trace_id,
                            request_started_at.elapsed().as_secs_f64(),
                            body.len() as f64 / (1024.0 * 1024.0),
                            file_name,
                            path,
                            duration_sec
                        );
                        let json = format!(
                            "{{\"path\":{},\"duration\":{}}}",
                            serde_json::json!(path),
                            duration_sec
                        );
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
                    }
                    Err(error) => {
                        crate::log_info!(
                            "[AudioImport:{}][HTTP] failed after {:.3}s: {}",
                            trace_id,
                            request_started_at.elapsed().as_secs_f64(),
                            error
                        );
                        let _ = std::fs::remove_file(&output_path);
                        let mut res = Response::from_string(error).with_status_code(500);
                        res.add_header(cors);
                        let _ = request.respond(res);
                    }
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

            // Extract the Range header value now (before moving `request` into the thread).
            let range_header_str: Option<String> = request
                .headers()
                .iter()
                .find(|h| h.field.to_string().eq_ignore_ascii_case("range"))
                .and_then(|h| h.value.as_str().strip_prefix("bytes="))
                .map(|s| s.to_owned());

            streaming::spawn_media_file_response(request, media_path_str, range_header_str);
        }
    });

    Ok(actual_port)
}
