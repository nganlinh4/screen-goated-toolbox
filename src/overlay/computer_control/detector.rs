//! Optional LOCAL ONNX UI-element detector, used ONLY on UIA-blind surfaces (canvas,
//! games, custom-drawn / Electron UIs) where the accessibility tree gives us nothing.
//! It produces class-agnostic "clickable region" boxes that become `click_mark`
//! anchors — so on those surfaces the model clicks a numbered mark instead of
//! pixel-guessing, while NORMAL (UIA-rich) apps are untouched.
//!
//! Model: racineai/UI-DETR-1 (MIT) — an RF-DETR-M (Apache) finetune. Produce the
//! `.onnx` once with `scripts/export_ui_detr.py` and drop it at
//! `%APPDATA%/screen-goated-toolbox/models/ui-detector/ui-detr-1.onnx`.
//!
//! Graceful by design: if the model file is absent (the default), everything here is
//! a no-op and the feature is simply off. Runs on DirectML (CPU fallback), gated to
//! blind surfaces so its cost is occasional, never per-turn.
//!
//! I/O (verified against the real `.onnx`): input `input` [1,3,1024,1024]
//! ImageNet-normalized; outputs `dets` [1,N,4] cxcywh-normalized + `labels` [1,N,C]
//! logits. Outputs are looked up by name and `try_extract` is fallible, so an
//! unexpected export degrades to a clean error (detector off) rather than a panic.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use sha2::{Digest, Sha256};

use super::session::{Capture, View};
pub(super) use postprocess::DetBox;
use postprocess::{PostprocessResult, postprocess};

mod postprocess;
mod runtime;
mod selection;
pub(super) use selection::select_marks;

/// Model input side length. MUST match the exported ONNX (UI-DETR-1 / RF-DETR-M,
/// exported at 1024 — a ÷32 resolution). Verified against the real model's
/// `input [1,3,1024,1024]`.
const RES: usize = 1024;
/// Max marks surfaced (strongest first) — keep the list readable for the model.
const MAX_CANDIDATES: usize = 90;
pub(super) const DISPLAY_MARKS: usize = 30;
/// ImageNet normalization (RF-DETR preprocessing).
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

fn score_threshold() -> f32 {
    probability_env("CC_DETECTOR_THRESH", 0.70)
}

fn nms_iou_threshold() -> f32 {
    probability_env("CC_DETECTOR_NMS_IOU", 0.92)
}

fn probability_env(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .unwrap_or(default)
}

/// Models dir for the detector (a single `.onnx` lives here). Doubles as the
/// "model dir" the Downloaded Tools card sizes/removes.
pub(crate) fn detector_model_dir() -> std::path::PathBuf {
    crate::paths::app_models_dir().join("ui-detector")
}

fn model_path() -> std::path::PathBuf {
    detector_model_dir().join("ui-detr-1.onnx")
}

/// Hosted on the SGT runtime-bundles release; fetched once into the models dir.
const MODEL_URL: &str = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/ui-detr-1.onnx";
const MODEL_BYTES: u64 = 131_216_489;
const MODEL_SHA256: &str = "1892092320cd55fd182c6afd76ae5bb0fb9695f5fcdf0ba875c1f68d49792ff4";
/// Title used in REALTIME_STATE + the badge; the Downloaded Tools card matches it.
pub(crate) const DOWNLOAD_TITLE: &str = "Downloading UI detector";

pub(crate) fn is_detector_downloaded() -> bool {
    validate_model_file(&model_path()).is_ok()
}

fn runtime_ready() -> bool {
    crate::unpack_dlls::is_ai_runtime_installed()
}

fn validate_model_file(path: &std::path::Path) -> anyhow::Result<()> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| anyhow::anyhow!("detector model unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != MODEL_BYTES {
        anyhow::bail!(
            "detector model size {} does not match expected {MODEL_BYTES}",
            metadata.len()
        );
    }
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    static CACHE: OnceLock<Mutex<Option<(u64, u128, bool)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some((bytes, modified, valid)) = *cache.lock().unwrap_or_else(|p| p.into_inner())
        && bytes == metadata.len()
        && modified == modified_ms
    {
        return if valid {
            Ok(())
        } else {
            anyhow::bail!("detector model checksum mismatch");
        };
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        use std::io::Read as _;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let valid = format!("{:x}", hasher.finalize()) == MODEL_SHA256;
    *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((metadata.len(), modified_ms, valid));
    if !valid {
        anyhow::bail!("detector model checksum mismatch");
    }
    Ok(())
}

pub(crate) fn remove_detector_model() -> anyhow::Result<()> {
    let dir = detector_model_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Download the model with LIVE progress on both the auto-copy badge and the
/// Downloaded Tools card (via `REALTIME_STATE`), exactly like the TTS/ASR models.
/// Used by the manual card button AND the automatic first-use fetch.
pub(crate) fn download_detector_model(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    use crate::overlay::auto_copy_badge::{hide_progress_notification, show_progress_notification};
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let badge_title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_model_fmt,
        &[("name", badge.ui_detector_name)],
    );
    let badge_preparing = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_model_fmt,
        &[("name", badge.ui_detector_name)],
    );
    let _download_guard = DOWNLOAD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    crate::unpack_dlls::ensure_ai_runtime_installed(
        stop.clone(),
        if use_badge {
            crate::unpack_dlls::AiRuntimeUi::Badge
        } else {
            crate::unpack_dlls::AiRuntimeUi::None
        },
    )?;
    let path = model_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if path.exists() && validate_model_file(&path).is_err() {
        std::fs::remove_file(&path)?;
    }
    let msg = "Fetching UI element detector...";
    if let Ok(mut s) = REALTIME_STATE.lock() {
        s.is_downloading = true;
        s.download_title = DOWNLOAD_TITLE.to_string();
        s.download_message = msg.to_string();
        s.download_progress = 0.0;
    }
    if use_badge {
        show_progress_notification(&badge_title, &badge_preparing, 0.0);
    }

    let result = crate::api::realtime_audio::model_loader::download_file_with_progress(
        MODEL_URL,
        &path,
        &stop,
        |downloaded, total| {
            let progress = if total > 0 {
                (downloaded as f32 / total as f32) * 100.0
            } else {
                0.0
            };
            if let Ok(mut s) = REALTIME_STATE.lock() {
                s.download_progress = progress;
            }
            if use_badge {
                show_progress_notification(&badge_title, &badge_title, progress);
            }
        },
    )
    .and_then(|()| validate_model_file(&path));

    if let Ok(mut s) = REALTIME_STATE.lock() {
        s.is_downloading = false;
        s.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if use_badge {
        hide_progress_notification();
    }
    result.map_err(|e| anyhow::anyhow!("detector model download: {e}"))
}

/// Kick off a background download with bounded retry. A transient network failure
/// or a later model deletion must recover without restarting the application.
fn ensure_download() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static LAST_ATTEMPT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut last = LAST_ATTEMPT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if last.is_some_and(|attempt| attempt.elapsed() < std::time::Duration::from_secs(30)) {
        RUNNING.store(false, Ordering::Release);
        return;
    }
    *last = Some(Instant::now());
    drop(last);
    std::thread::spawn(|| {
        if !is_detector_downloaded() || !runtime_ready() {
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            match download_detector_model(stop, true) {
                Ok(()) => eprintln!("[cc-detector] model downloaded"),
                Err(error) => eprintln!("[cc-detector] auto-download failed: {error}"),
            }
        }
        RUNNING.store(false, Ordering::Release);
    });
}

/// Whether the detector model is installed. If absent, kicks off a one-time
/// background download from the release and returns false until it lands.
pub(super) fn available() -> bool {
    if is_detector_downloaded() && runtime_ready() {
        return true;
    }
    ensure_download();
    false
}

/// Model-only diagnostic: download/validate if needed, detect one requested
/// window (or the virtual desktop), and write the exact annotated input frame.
pub(crate) fn run_test(target: Option<&str>) -> anyhow::Result<()> {
    if !is_detector_downloaded() || !runtime_ready() {
        eprintln!("[cc-detector-test] installing validated model + runtime...");
        download_detector_model(
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
        )?;
    }
    let view = match target {
        Some(title) => {
            match super::uia::raise_window(title) {
                Ok(true) => {}
                Ok(false) => anyhow::bail!("target window {title:?} could not be foregrounded"),
                Err(error) => anyhow::bail!("cannot resolve target window {title:?}: {error}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(350));
            // `raise_window` accepts either a title or executable name and verifies
            // that its resolved HWND became foreground. Re-resolving `title` through
            // UIA here is a different (title-only) contract: an executable target
            // such as `sample.exe` can focus correctly and then spuriously be
            // reported missing. Crop the verified foreground window instead.
            super::uia::target_window_rect(None)
                .map(|(x, y, w, h)| View { x, y, w, h })
                .ok_or_else(|| anyhow::anyhow!("target window {title:?} not found"))?
        }
        None => {
            let (x, y, w, h) = super::uia::virtual_desktop();
            View { x, y, w, h }
        }
    };
    super::telemetry::begin_session();
    let trace_dir = super::telemetry::trace_dir();
    std::fs::create_dir_all(&trace_dir)?;
    let capture = super::session::capture_virtual()?;
    let frame_id = super::telemetry::next_frame("detector_test");
    let boxes = selection::select_marks(
        detect_capture_result(&capture, view, frame_id)?,
        view,
        DISPLAY_MARKS,
    );
    let marks: Vec<_> = boxes
        .iter()
        .enumerate()
        .map(|(index, item)| (item.cx, item.cy, index as u32 + 1))
        .collect();
    let (jpeg, shown) =
        super::session::encode_view(&capture, view, 1080, None, None, Some(&marks))?;
    let path = trace_dir.join("detector-test-annotated.jpg");
    std::fs::write(&path, jpeg)?;
    let evidence: Vec<_> = boxes
        .iter()
        .enumerate()
        .map(|(index, item)| {
            serde_json::json!({
                "mark": index + 1,
                "center": [item.cx, item.cy],
                "bounds": [item.left, item.top, item.right, item.bottom],
                "score": item.score,
                "label": item.label,
            })
        })
        .collect();
    let result = serde_json::json!({
        "ok": true,
        "target": target,
        "view": [shown.x, shown.y, shown.w, shown.h],
        "model_bytes": std::fs::metadata(model_path())?.len(),
        "mark_count": boxes.len(),
        "requested_execution_provider": runtime::requested_provider(),
        "actual_execution_provider": runtime::actual_provider(),
        "score_threshold": score_threshold(),
        "duplicate_iou_threshold": nms_iou_threshold(),
        "marks": evidence,
        "annotated_frame": path,
        "trace_dir": trace_dir,
    });
    let result_text = serde_json::to_string_pretty(&result)?;
    std::fs::write(trace_dir.join("detector-test.json"), &result_text)?;
    println!("{result_text}");
    Ok(())
}

/// True only for an effectively black capture failure. A visually sparse or flat
/// application is still valid detector input and must not be discarded.
fn crop_capture_failed(img: &image::RgbImage) -> bool {
    if img.width() == 0 || img.height() == 0 {
        return true;
    }
    img.pixels()
        .all(|pixel| pixel.0.iter().all(|value| *value < 2))
}

/// Run the detector against the exact clean capture that will be annotated and
/// sent to the model. This keeps every numbered mark tied to one frame.
pub(super) fn detect_capture(cap: &Capture, view: View, frame_id: u64) -> Vec<DetBox> {
    match detect_capture_result(cap, view, frame_id) {
        Ok(boxes) => boxes,
        Err(error) => {
            eprintln!("[cc-detector] inference error: {error}");
            super::telemetry::typed_error(
                "ERR_UI_DETECTOR_INFERENCE",
                "detector",
                &error.to_string(),
                serde_json::json!({"frame_id": frame_id, "view": [view.x, view.y, view.w, view.h]}),
            );
            Vec::new()
        }
    }
}

fn detect_capture_result(cap: &Capture, view: View, frame_id: u64) -> anyhow::Result<Vec<DetBox>> {
    let started = Instant::now();
    let (cw, ch) = (cap.rgb.width() as i32, cap.rgb.height() as i32);
    // Clamp the view rect into the captured buffer (capture is origin-relative).
    let x0 = (view.x - cap.origin_x).clamp(0, cw);
    let y0 = (view.y - cap.origin_y).clamp(0, ch);
    let x1 = (view.x + view.w - cap.origin_x).clamp(0, cw);
    let y1 = (view.y + view.h - cap.origin_y).clamp(0, ch);
    if x1 <= x0 || y1 <= y0 {
        detector_event(frame_id, view, "view_outside_capture", started, None);
        anyhow::bail!("target view is outside the captured desktop");
    }
    let crop = image::imageops::crop_imm(
        &cap.rgb,
        x0 as u32,
        y0 as u32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    )
    .to_image();
    if crop_capture_failed(&crop) {
        detector_event(frame_id, view, "black_capture", started, None);
        return Ok(Vec::new());
    }
    detector_stage(frame_id, "crop_ready", started);
    // Screen-px origin of the crop's top-left.
    let (ox, oy) = (x0 + cap.origin_x, y0 + cap.origin_y);
    match run(&crop, ox, oy, frame_id) {
        Ok(processed) => {
            detector_event(frame_id, view, "complete", started, Some(&processed));
            Ok(processed.boxes)
        }
        Err(e) => {
            detector_event(frame_id, view, "inference_error", started, None);
            Err(e)
        }
    }
}

fn detector_stage(frame_id: u64, stage: &'static str, started: Instant) {
    super::telemetry::event(
        "detector_stage",
        "detector",
        super::telemetry::Privacy::Safe,
        serde_json::json!({
            "frame_id": frame_id,
            "stage": stage,
            "elapsed_ms": started.elapsed().as_millis(),
            "requested_execution_provider": runtime::requested_provider(),
        }),
    );
}

fn detector_event(
    frame_id: u64,
    view: View,
    outcome: &str,
    started: Instant,
    processed: Option<&PostprocessResult>,
) {
    let boxes = processed.map_or(&[][..], |value| value.boxes.as_slice());
    let evidence: Vec<_> = boxes
        .iter()
        .map(|item| {
            serde_json::json!({
                "center": [item.cx, item.cy],
                "bounds": [item.left, item.top, item.right, item.bottom],
                "score": item.score,
                "label": item.label,
            })
        })
        .collect();
    super::telemetry::event(
        "detector_run",
        "detector",
        if boxes.iter().any(|item| item.label.is_some()) {
            super::telemetry::Privacy::UserText
        } else {
            super::telemetry::Privacy::Safe
        },
        serde_json::json!({
            "provider": "local_ui_detr_1",
            "requested_execution_provider": runtime::requested_provider(),
            "actual_execution_provider": runtime::actual_provider(),
            "score_threshold": score_threshold(),
            "duplicate_iou_threshold": nms_iou_threshold(),
            "frame_id": frame_id,
            "view": [view.x, view.y, view.w, view.h],
            "outcome": outcome,
            "duration_ms": started.elapsed().as_millis(),
            "thresholded": processed.map(|value| value.thresholded),
            "rejected_invalid": processed.map(|value| value.rejected_invalid),
            "suppressed_duplicates": processed.map(|value| value.suppressed_duplicates),
            "truncated": processed.map(|value| value.truncated),
            "mark_count": boxes.len(),
            "marks": evidence,
        }),
    );
}

fn run(
    crop: &image::RgbImage,
    ox: i32,
    oy: i32,
    frame_id: u64,
) -> anyhow::Result<PostprocessResult> {
    let started = Instant::now();
    crate::unpack_dlls::ensure_onnx_runtime_initialized()?;
    detector_stage(frame_id, "runtime_ready", started);
    let (cw, ch) = (crop.width() as f32, crop.height() as f32);

    // Square resize to RES×RES + ImageNet normalize → NCHW f32 (matches RF-DETR's
    // transform; boxes come back normalized so the square distortion is undone by
    // scaling back to the crop's own width/height).
    let resized = image::imageops::resize(
        crop,
        RES as u32,
        RES as u32,
        image::imageops::FilterType::Triangle,
    );
    detector_stage(frame_id, "resize_ready", started);
    let plane = RES * RES;
    let mut chw = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        for c in 0..3 {
            chw[c * plane + i] = (px[c] as f32 / 255.0 - MEAN[c]) / STD[c];
        }
    }
    detector_stage(frame_id, "tensor_data_ready", started);
    let input = ort::value::Value::from_array((vec![1i64, 3, RES as i64, RES as i64], chw))
        .map_err(|e| anyhow::anyhow!("input tensor: {e}"))?;
    detector_stage(frame_id, "input_ready", started);

    let processed = runtime::with_session(&model_path(), move |session| {
        detector_stage(frame_id, "session_ready", started);
        // Outputs borrow the session, so extraction and postprocessing stay inside
        // the cache lock. One detector run is serialized by design.
        let outputs = session
            .run(ort::inputs!["input" => input])
            .map_err(|error| anyhow::anyhow!("run: {error}"))?;
        detector_stage(frame_id, "inference_ready", started);
        let dets_v = outputs
            .get("dets")
            .ok_or_else(|| anyhow::anyhow!("model has no 'dets' output"))?;
        let labels_v = outputs
            .get("labels")
            .ok_or_else(|| anyhow::anyhow!("model has no 'labels' output"))?;
        let (dshape, dets) = dets_v
            .try_extract_tensor::<f32>()
            .map_err(|error| anyhow::anyhow!("dets: {error}"))?;
        let (lshape, labels) = labels_v
            .try_extract_tensor::<f32>()
            .map_err(|error| anyhow::anyhow!("labels: {error}"))?;
        let processed = postprocess(
            dshape.as_ref(),
            dets,
            lshape.as_ref(),
            labels,
            cw,
            ch,
            ox,
            oy,
            score_threshold(),
            nms_iou_threshold(),
        )?;
        detector_stage(frame_id, "postprocess_ready", started);
        Ok(processed)
    })?;
    eprintln!(
        "[cc-detector] inference {}ms: {} candidates over threshold, {} invalid, {} duplicates suppressed, {} truncated, {} marks",
        started.elapsed().as_millis(),
        processed.thresholded,
        processed.rejected_invalid,
        processed.suppressed_duplicates,
        processed.truncated,
        processed.boxes.len()
    );
    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_effectively_black_capture_is_skipped() {
        let black = image::RgbImage::from_pixel(64, 64, image::Rgb([0, 0, 0]));
        assert!(crop_capture_failed(&black));
        let dark = image::RgbImage::from_pixel(64, 64, image::Rgb([8, 8, 8]));
        assert!(!crop_capture_failed(&dark));
        let light = image::RgbImage::from_pixel(64, 64, image::Rgb([240, 240, 240]));
        assert!(!crop_capture_failed(&light));
        let mut sparse = image::RgbImage::from_pixel(128, 128, image::Rgb([0, 0, 0]));
        sparse.put_pixel(17, 43, image::Rgb([255, 255, 255]));
        assert!(!crop_capture_failed(&sparse));
    }
}
