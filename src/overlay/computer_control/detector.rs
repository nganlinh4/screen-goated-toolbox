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
//! NOTE: the inference is written to RF-DETR's documented I/O (input `input`
//! [1,3,RES,RES]; outputs `dets` [1,N,4] cxcywh-normalized + `labels` [1,N,C] logits)
//! and the repo's `ort` 2.0 conventions, but the exact numerics (input name,
//! normalization, resolution divisibility) need one verification pass against the
//! real `.onnx` on a genuinely UIA-blind surface.

use std::sync::{Mutex, OnceLock};

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

use super::session::{View, capture_virtual};

/// Model input side length. MUST match the exported ONNX (UI-DETR-1 / RF-DETR-M,
/// exported at 1024 — a ÷32 resolution). Verified against the real model's
/// `input [1,3,1024,1024]`.
const RES: usize = 1024;
/// Max marks surfaced (strongest first) — keep the list readable for the model.
const MAX_MARKS: usize = 60;
/// ImageNet normalization (RF-DETR preprocessing).
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

fn score_threshold() -> f32 {
    std::env::var("CC_DETECTOR_THRESH").ok().and_then(|s| s.parse().ok()).unwrap_or(0.45)
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
const MODEL_URL: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/ui-detr-1.onnx";
/// Title used in REALTIME_STATE + the badge; the Downloaded Tools card matches it.
pub(crate) const DOWNLOAD_TITLE: &str = "Downloading UI detector";

pub(crate) fn is_detector_downloaded() -> bool {
    model_path().exists()
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

    let path = model_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let msg = "Fetching UI element detector...";
    if let Ok(mut s) = REALTIME_STATE.lock() {
        s.is_downloading = true;
        s.download_title = DOWNLOAD_TITLE.to_string();
        s.download_message = msg.to_string();
        s.download_progress = 0.0;
    }
    if use_badge {
        show_progress_notification(DOWNLOAD_TITLE, msg, 0.0);
    }

    let result = crate::api::realtime_audio::model_loader::download_file_with_progress(
        MODEL_URL,
        &path,
        &stop,
        |downloaded, total| {
            let progress = if total > 0 { (downloaded as f32 / total as f32) * 100.0 } else { 0.0 };
            if let Ok(mut s) = REALTIME_STATE.lock() {
                s.download_progress = progress;
            }
            if use_badge {
                show_progress_notification(DOWNLOAD_TITLE, "Downloading UI element detector...", progress);
            }
        },
    );

    if let Ok(mut s) = REALTIME_STATE.lock() {
        s.is_downloading = false;
        s.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if use_badge {
        hide_progress_notification();
    }
    result.map_err(|e| anyhow::anyhow!("detector model download: {e}"))
}

/// Kick off the model download ONCE in the background (with badge + card progress)
/// so it never blocks a turn; until it lands `available()` stays false and the
/// detector is simply off.
fn ensure_download() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            if model_path().exists() {
                return;
            }
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            match download_detector_model(stop, true) {
                Ok(()) => eprintln!("[cc-detector] model downloaded"),
                Err(e) => eprintln!("[cc-detector] auto-download failed: {e}"),
            }
        });
    });
}

/// A detected clickable region: center in physical SCREEN px + confidence.
pub(super) struct DetBox {
    pub cx: i32,
    pub cy: i32,
    pub score: f32,
}

/// Whether the detector model is installed. If absent, kicks off a one-time
/// background download from the release and returns false until it lands.
pub(super) fn available() -> bool {
    if model_path().exists() {
        return true;
    }
    ensure_download();
    false
}

/// Lazily build the session once. `None` when the model is absent or fails to load.
fn session() -> Option<&'static Mutex<Session>> {
    static S: OnceLock<Option<Mutex<Session>>> = OnceLock::new();
    S.get_or_init(|| {
        let path = model_path();
        if !path.exists() {
            return None;
        }
        match build_session(&path) {
            Ok(s) => {
                eprintln!("[cc-detector] loaded {}", path.display());
                Some(Mutex::new(s))
            }
            Err(e) => {
                eprintln!("[cc-detector] load failed ({}): {e}", path.display());
                None
            }
        }
    })
    .as_ref()
}

fn build_session(path: &std::path::Path) -> anyhow::Result<Session> {
    // ort's error type isn't Send+Sync, so it can't flow through `?` into anyhow -
    // stringify each step (the repo's parakeet path does the same).
    let mut builder = Session::builder()
        .map_err(|e| anyhow::anyhow!("session builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow::anyhow!("opt level: {e}"))?
        .with_execution_providers([
            ort::ep::DirectML::default().build(),
            ort::ep::CPU::default().build().error_on_failure(),
        ])
        .map_err(|e| anyhow::anyhow!("execution providers: {e}"))?;
    builder.commit_from_file(path).map_err(|e| anyhow::anyhow!("commit model: {e}"))
}

/// True when the image is so close to one flat colour (e.g. all-black GPU content
/// the capture missed) that running detection is pointless.
fn crop_near_uniform(img: &image::RgbImage) -> bool {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return true;
    }
    let (sx, sy) = ((w / 32).max(1), (h / 32).max(1));
    let (mut sum, mut sumsq, mut n) = (0f64, 0f64, 0f64);
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let p = img.get_pixel(x, y);
            let luma = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
            sum += luma;
            sumsq += luma * luma;
            n += 1.0;
            x += sx;
        }
        y += sy;
    }
    let mean = sum / n;
    (sumsq / n - mean * mean) < 25.0 // std-dev < 5 luma => effectively flat
}

/// Capture the current `view`, run the detector, and return clickable-region centers
/// in SCREEN px. Empty if no model / capture fails / nothing found.
pub(super) fn detect_view(view: View) -> Vec<DetBox> {
    if session().is_none() {
        return Vec::new();
    }
    let Ok(cap) = capture_virtual() else {
        return Vec::new();
    };
    let (cw, ch) = (cap.rgb.width() as i32, cap.rgb.height() as i32);
    // Clamp the view rect into the captured buffer (capture is origin-relative).
    let x0 = (view.x - cap.origin_x).clamp(0, cw);
    let y0 = (view.y - cap.origin_y).clamp(0, ch);
    let x1 = (view.x + view.w - cap.origin_x).clamp(0, cw);
    let y1 = (view.y + view.h - cap.origin_y).clamp(0, ch);
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let crop =
        image::imageops::crop_imm(&cap.rgb, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
            .to_image();
    // Skip inference on a near-uniform/black crop (GPU content the capture still
    // couldn't grab) - it would only emit noise and wastes ~1s.
    if crop_near_uniform(&crop) {
        return Vec::new();
    }
    // Screen-px origin of the crop's top-left.
    let (ox, oy) = (x0 + cap.origin_x, y0 + cap.origin_y);
    match run(&crop, ox, oy) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[cc-detector] inference error: {e}");
            Vec::new()
        }
    }
}

fn run(crop: &image::RgbImage, ox: i32, oy: i32) -> anyhow::Result<Vec<DetBox>> {
    let sess = session().ok_or_else(|| anyhow::anyhow!("no detector session"))?;
    let (cw, ch) = (crop.width() as f32, crop.height() as f32);

    // Square resize to RES×RES + ImageNet normalize → NCHW f32 (matches RF-DETR's
    // transform; boxes come back normalized so the square distortion is undone by
    // scaling back to the crop's own width/height).
    let resized = image::imageops::resize(crop, RES as u32, RES as u32, image::imageops::FilterType::Triangle);
    let plane = RES * RES;
    let mut chw = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        for c in 0..3 {
            chw[c * plane + i] = (px[c] as f32 / 255.0 - MEAN[c]) / STD[c];
        }
    }
    let input = ort::value::Value::from_array((vec![1i64, 3, RES as i64, RES as i64], chw))
        .map_err(|e| anyhow::anyhow!("input tensor: {e}"))?;

    // Hold the lock through extraction: the output tensors borrow the session guard.
    let mut s = sess.lock().unwrap();
    let outputs = s.run(ort::inputs!["input" => input]).map_err(|e| anyhow::anyhow!("run: {e}"))?;
    let (dshape, dets) = outputs["dets"]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow::anyhow!("dets: {e}"))?; // [1, N, 4] cxcywh 0..1
    let (lshape, labels) = outputs["labels"]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow::anyhow!("labels: {e}"))?; // [1, N, C] logits
    let dd = dshape.as_ref();
    let ld = lshape.as_ref();
    let n = if dd.len() >= 2 { dd[1] as usize } else { 0 };
    let nc = if ld.len() >= 3 { ld[2].max(1) as usize } else { 1 };

    let thr = score_threshold();
    let mut out = Vec::new();
    for i in 0..n {
        // Class-agnostic: best class logit → sigmoid → confidence.
        let mut best = f32::MIN;
        for c in 0..nc {
            best = best.max(labels[i * nc + c]);
        }
        let score = 1.0 / (1.0 + (-best).exp());
        if score < thr {
            continue;
        }
        let cxn = dets[i * 4];
        let cyn = dets[i * 4 + 1];
        let sx = ox + (cxn * cw).round() as i32;
        let sy = oy + (cyn * ch).round() as i32;
        out.push(DetBox { cx: sx, cy: sy, score });
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.truncate(MAX_MARKS);
    Ok(out)
}
