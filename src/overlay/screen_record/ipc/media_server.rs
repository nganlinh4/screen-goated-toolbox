//! Process-wide loopback media server for the recorder WebView: range-served
//! playback of local files plus upload/import endpoints, behind a token gate.

mod audio_import;
mod body_io;
mod gate;
mod media_probe;
mod routes;
mod streaming;
mod video_import;

use super::super::{MEDIA_SERVER_TOKEN, SERVER_PORT};

use self::gate::{check_request_gate, mint_media_server_token};

pub use self::audio_import::{
    create_audio_placeholder_video, import_audio_path_to_managed_media_file,
};
pub use self::video_import::import_video_path_to_managed_media_file;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use tiny_http::{Response, Server};

/// Concurrent request handlers; further requests are answered 503 at once.
const MAX_ACTIVE_HANDLERS: usize = 64;

static START_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_HANDLERS: AtomicUsize = AtomicUsize::new(0);
static IMPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn recordings_dir() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("recordings")
}

/// Unique, time-ordered file-name stamp for managed imports.
fn unique_import_stamp() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = IMPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{ts}-{}-{sequence}", std::process::id())
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

/// Starts the media server once per process and returns its port. Later calls
/// (every editor window reopen) reuse the running server.
pub fn start_global_media_server() -> Result<u16, String> {
    let _start = START_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let running = SERVER_PORT.load(Ordering::SeqCst);
    if running != 0 {
        return Ok(running);
    }
    if MEDIA_SERVER_TOKEN.get().is_none() {
        MEDIA_SERVER_TOKEN
            .set(mint_media_server_token()?)
            .map_err(|_| "Media-server token was initialized concurrently".to_string())?;
    }

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

    thread::Builder::new()
        .name("sgt-media-server".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                accept(request);
            }
            SERVER_PORT.store(0, Ordering::SeqCst);
        })
        .map_err(|error| format!("Start media server thread: {error}"))?;
    SERVER_PORT.store(port, Ordering::SeqCst);
    Ok(port)
}

/// Answers cheap requests inline and hands everything else to a handler
/// thread, so uploads, probes, and file streams never queue behind each other.
fn accept(request: tiny_http::Request) {
    if request.method() == &tiny_http::Method::Options {
        let mut res = Response::empty(204);
        res.add_header(routes::cors_header());
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
                &b"Range, Content-Type, X-SGT-Token"[..],
            )
            .unwrap(),
        );
        let _ = request.respond(res);
        return;
    }
    if let Err(rejection) = check_request_gate(&request) {
        routes::respond_text(request, 403, rejection.body);
        return;
    }
    if ACTIVE_HANDLERS.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE_HANDLERS {
        ACTIVE_HANDLERS.fetch_sub(1, Ordering::SeqCst);
        routes::respond_text(request, 503, "Media server is busy");
        return;
    }
    let spawned = thread::Builder::new()
        .name("sgt-media-request".into())
        .spawn(move || {
            struct Slot;
            impl Drop for Slot {
                fn drop(&mut self) {
                    ACTIVE_HANDLERS.fetch_sub(1, Ordering::SeqCst);
                }
            }
            let _slot = Slot;
            routes::handle(request);
        });
    if spawned.is_err() {
        ACTIVE_HANDLERS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "media_server_tests.rs"]
mod tests;
