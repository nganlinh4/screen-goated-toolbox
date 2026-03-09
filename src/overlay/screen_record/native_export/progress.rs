use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::SR_HWND;
use serde::Serialize;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_APP_RUN_SCRIPT: u32 = WM_USER + 112;
const REPLAY_INLINE_MEDIA_MAX_BYTES: usize = 8 * 1024 * 1024;

pub fn export_replay_args_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| {
        base.join("screen-goated-toolbox")
            .join("export-debug")
            .join("last_export_args.json")
    })
}

pub fn export_result_log_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| {
        base.join("screen-goated-toolbox")
            .join("export-debug")
            .join("last_export_result.json")
    })
}

pub fn persist_export_result(result: &Result<serde_json::Value, String>) {
    let Some(path) = export_result_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let log_value = match result {
        Ok(v) => serde_json::json!({
            "outcome": "ok",
            "result": v,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }),
        Err(e) => serde_json::json!({
            "outcome": "error",
            "error": e,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }),
    };
    if let Ok(bytes) = serde_json::to_vec_pretty(&log_value) {
        let _ = fs::write(path, bytes);
    }
}

pub fn persist_replay_args(args: &serde_json::Value) {
    let Some(path) = export_replay_args_path() else {
        return;
    };
    let Some(obj) = args.as_object() else {
        return;
    };

    let source_video_path = obj
        .get("sourceVideoPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let inline_video_len = obj
        .get("videoData")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    let inline_audio_len = obj
        .get("audioData")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);

    let has_source_path = !source_video_path.is_empty();
    let should_strip_inline_media = has_source_path
        || inline_video_len > REPLAY_INLINE_MEDIA_MAX_BYTES
        || inline_audio_len > REPLAY_INLINE_MEDIA_MAX_BYTES;

    let mut replay_obj = serde_json::Map::with_capacity(obj.len() + 1);
    for (key, value) in obj {
        if should_strip_inline_media && (key == "videoData" || key == "audioData") {
            replay_obj.insert(key.clone(), serde_json::Value::Null);
        } else {
            replay_obj.insert(key.clone(), value.clone());
        }
    }
    replay_obj.insert(
        "_replayMeta".to_string(),
        serde_json::json!({
            "savedAtMs": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            "inlineMediaStripped": should_strip_inline_media
        }),
    );

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&serde_json::Value::Object(replay_obj)) {
        let _ = fs::write(path, bytes);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressUpdate {
    pub percent: f64,
    pub eta: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_name: Option<String>,
}

/// Push a progress update directly to the WebView via PostMessageW.
/// This avoids IPC round-trips and works even while another invoke is pending.
pub fn push_export_progress_update(update: ExportProgressUpdate) {
    let payload = serde_json::json!({
        "type": "sr-export-progress",
        "percent": update.percent,
        "eta": update.eta,
        "phase": update.phase,
        "clipIndex": update.clip_index,
        "clipCount": update.clip_count,
        "clipName": update.clip_name,
    });
    let script = format!("window.postMessage({},'*')", payload);
    let script_ptr = Box::into_raw(Box::new(script));
    let hwnd = unsafe { std::ptr::addr_of!(SR_HWND).read() };
    if !hwnd.0.is_invalid() {
        unsafe {
            let _ = PostMessageW(
                Some(hwnd.0),
                WM_APP_RUN_SCRIPT,
                WPARAM(0),
                LPARAM(script_ptr as isize),
            );
        }
    } else {
        unsafe {
            drop(Box::from_raw(script_ptr));
        }
    }
}

pub fn push_export_progress(percent: f64, eta: f64) {
    push_export_progress_update(ExportProgressUpdate {
        percent,
        eta,
        phase: None,
        clip_index: None,
        clip_count: None,
        clip_name: None,
    });
}
