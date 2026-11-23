//! HTTP route handlers. Each runs on its own handler thread, so a long upload
//! or probe never blocks playback requests.

use std::time::Instant;

use tiny_http::{Request, Response};

use super::audio_import::{audio_import_extension, import_audio_into_managed};
use super::body_io::{
    MAX_IMPORTED_AUDIO_BYTES, MAX_IMPORTED_VIDEO_BYTES, MAX_RESTORED_VIDEO_BYTES,
    write_body_bounded,
};
use super::gate::query_param;
use super::video_import::{import_video_into_managed, video_import_extension};
use super::{recordings_dir, streaming, unique_import_stamp};

pub(super) fn cors_header() -> tiny_http::Header {
    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
}

pub(super) fn respond_text(request: Request, status: u16, body: impl Into<String>) {
    let mut res = Response::from_string(body.into()).with_status_code(status);
    res.add_header(cors_header());
    let _ = request.respond(res);
}

fn respond_json(request: Request, body: serde_json::Value) {
    let mut res = Response::from_string(body.to_string()).with_status_code(200);
    res.add_header(cors_header());
    res.add_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
    );
    let _ = request.respond(res);
}

pub(super) fn handle(request: Request) {
    let is_post = request.method() == &tiny_http::Method::Post;
    let url = request.url().to_string();
    if is_post && url.starts_with("/write-temp") {
        write_temp(request);
    } else if is_post && url.starts_with("/import-video") {
        import_video(request, &url);
    } else if is_post && url.starts_with("/import-audio") {
        import_audio(request, &url);
    } else {
        let media_path = query_param(&url, "path").unwrap_or_default();
        if media_path.is_empty() {
            respond_text(request, 404, "File not found");
            return;
        }
        let range = request
            .headers()
            .iter()
            .find(|h| h.field.to_string().eq_ignore_ascii_case("range"))
            .and_then(|h| h.value.as_str().strip_prefix("bytes="))
            .map(str::to_owned);
        streaming::respond_media_file(request, &media_path, range.as_deref());
    }
}

fn write_temp(mut request: Request) {
    let recordings_dir = recordings_dir();
    if let Err(error) = std::fs::create_dir_all(&recordings_dir) {
        respond_text(
            request,
            500,
            format!("Create recordings directory failed: {error}"),
        );
        return;
    }
    let dest = recordings_dir.join(format!("restored_{}.mp4", unique_import_stamp()));
    match write_body_bounded(request.as_reader(), &dest, MAX_RESTORED_VIDEO_BYTES) {
        Ok(_) => respond_json(
            request,
            serde_json::json!({ "path": dest.to_string_lossy() }),
        ),
        Err(_) => respond_text(request, 500, "Write failed"),
    }
}

fn upload_names(url: &str, default_name: &str, kind: &str) -> (String, String) {
    let file_name = query_param(url, "filename").unwrap_or_else(|| default_name.to_string());
    let trace_id = query_param(url, "traceId")
        .unwrap_or_else(|| format!("{kind}-import-{}", unique_import_stamp()));
    (file_name, trace_id)
}

fn import_video(mut request: Request, url: &str) {
    let (file_name, trace_id) = upload_names(url, "upload.mp4", "video");
    let started_at = Instant::now();
    let result = video_import_extension(&file_name).and_then(|extension| {
        import_video_into_managed(extension, |output| {
            write_body_bounded(request.as_reader(), output, MAX_IMPORTED_VIDEO_BYTES)
        })
    });
    match result {
        Ok(imported) => {
            crate::log_info!(
                "[VideoImport:{}][HTTP] complete total {:.3}s size_mb={:.2} file=\"{}\" output=\"{}\" has_audio={}",
                trace_id,
                started_at.elapsed().as_secs_f64(),
                imported.bytes as f64 / (1024.0 * 1024.0),
                file_name,
                imported.path.display(),
                imported.has_audio
            );
            respond_json(
                request,
                serde_json::json!({
                    "path": imported.path.to_string_lossy(),
                    "hasAudio": imported.has_audio,
                }),
            );
        }
        Err(error) => {
            crate::log_info!(
                "[VideoImport:{}][HTTP] failed after {:.3}s: {}",
                trace_id,
                started_at.elapsed().as_secs_f64(),
                error
            );
            respond_text(request, 500, error);
        }
    }
}

fn import_audio(mut request: Request, url: &str) {
    let (file_name, trace_id) = upload_names(url, "upload.mp3", "audio");
    let started_at = Instant::now();
    let result = audio_import_extension(&file_name).and_then(|extension| {
        import_audio_into_managed(extension, &trace_id, |output| {
            write_body_bounded(request.as_reader(), output, MAX_IMPORTED_AUDIO_BYTES)
        })
    });
    match result {
        Ok(imported) => {
            crate::log_info!(
                "[AudioImport:{}][HTTP] complete total {:.3}s size_mb={:.2} file=\"{}\" output=\"{}\" duration={:.3}s",
                trace_id,
                started_at.elapsed().as_secs_f64(),
                imported.bytes as f64 / (1024.0 * 1024.0),
                file_name,
                imported.path.display(),
                imported.duration_sec
            );
            respond_json(
                request,
                serde_json::json!({
                    "path": imported.path.to_string_lossy(),
                    "duration": imported.duration_sec,
                }),
            );
        }
        Err(error) => {
            crate::log_info!(
                "[AudioImport:{}][HTTP] failed after {:.3}s: {}",
                trace_id,
                started_at.elapsed().as_secs_f64(),
                error
            );
            respond_text(request, 500, error);
        }
    }
}
