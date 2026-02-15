use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, TryRecvError};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use super::gpu_export::{create_uniforms, CompositorUniforms, GpuCompositor};
use super::SR_HWND;
use crate::overlay::screen_record::engine::VIDEO_PATH;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_APP_RUN_SCRIPT: u32 = WM_USER + 112;
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Flag to signal export cancellation from the frontend.
static EXPORT_CANCELLED: AtomicBool = AtomicBool::new(false);
/// PIDs of the running decoder/encoder so cancel can kill them to unblock IO.
static EXPORT_PIDS: Mutex<(u32, u32)> = Mutex::new((0, 0));
/// Ensures GPU export warm-up runs once per app session.
static EXPORT_GPU_WARMED: AtomicBool = AtomicBool::new(false);
/// Indicates an export is actively running.
static EXPORT_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Cached check for ffmpeg NVENC support.
static NVENC_AVAILABLE: OnceLock<bool> = OnceLock::new();
/// Cache source video dimensions by path+mtime+size to avoid repeated ffprobe.
static SOURCE_DIM_CACHE: OnceLock<Mutex<HashMap<String, (u32, u32)>>> = OnceLock::new();

struct ExportActiveGuard;

impl ExportActiveGuard {
    fn activate() -> Self {
        EXPORT_ACTIVE.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for ExportActiveGuard {
    fn drop(&mut self) {
        EXPORT_ACTIVE.store(false, Ordering::SeqCst);
    }
}

pub fn cancel_export() {
    println!("[Cancel] Setting EXPORT_CANCELLED flag");
    EXPORT_CANCELLED.store(true, Ordering::SeqCst);
    let (dec_pid, enc_pid) = *EXPORT_PIDS.lock().unwrap();
    println!("[Cancel] PIDs: decoder={}, encoder={}", dec_pid, enc_pid);
    terminate_pid(dec_pid);
    terminate_pid(enc_pid);
    println!("[Cancel] Kill commands sent");
}

fn terminate_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    // Use Windows taskkill to forcefully terminate the ffmpeg process tree
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Push a progress update directly to the WebView via PostMessageW.
/// This avoids IPC round-trips and works even while another invoke is pending.
fn push_export_progress(percent: f64, eta: f64) {
    let script = format!(
        "window.postMessage({{type:'sr-export-progress',percent:{:.1},eta:{:.1}}},'*')",
        percent, eta
    );
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
        // No window — free the allocation
        unsafe {
            drop(Box::from_raw(script_ptr));
        }
    }
}

// --- Structs for JSON Deserialization ---

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportConfig {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub source_width: u32,
    #[serde(default)]
    pub source_height: u32,
    pub framerate: u32,
    pub audio_path: String,
    #[serde(default)]
    pub output_dir: String,
    pub trim_start: f64,
    pub duration: f64,
    pub speed: f64,
    pub segment: VideoSegment,
    pub background_config: BackgroundConfig,
    pub video_data: Option<Vec<u8>>,
    pub audio_data: Option<Vec<u8>>,
    pub baked_path: Option<Vec<BakedCameraFrame>>,
    pub baked_cursor_path: Option<Vec<BakedCursorFrame>>,
    #[serde(default)]
    pub baked_text_overlays: Vec<BakedTextOverlay>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BakedCameraFrame {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BakedCursorFrame {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub is_clicked: bool,
    #[serde(rename = "type")]
    pub cursor_type: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub rotation: f64,
}

#[derive(Debug, Clone)]
struct ParsedBakedCursorFrame {
    time: f64,
    x: f64,
    y: f64,
    scale: f64,
    type_id: f32,
    opacity: f64,
    rotation: f64,
}

pub fn warm_up_export_pipeline() {
    if EXPORT_GPU_WARMED.swap(true, Ordering::SeqCst) {
        println!("[Export][Warmup] already started/skipped");
        return;
    }
    if EXPORT_ACTIVE.load(Ordering::SeqCst) {
        println!("[Export][Warmup] export active, skipping warm-up");
        return;
    }

    let warmup_start = Instant::now();
    let warm_w = 1920u32;
    let warm_h = 1080u32;
    println!(
        "[Export][Warmup] starting GPU warm-up {}x{}",
        warm_w, warm_h
    );

    match GpuCompositor::new(warm_w, warm_h, warm_w, warm_h, warm_w, warm_h) {
        Ok(compositor) => {
            // Lightweight warm-up: avoid full 60-slot SVG atlas build here.
            // First export uses fast-partial slots; this keeps startup contention low.
            let _ = compositor.init_cursor_texture_fast(&[0]);

            let blank_frame = vec![0u8; (warm_w * warm_h * 4) as usize];
            compositor.upload_frame(&blank_frame);

            let uniforms = create_uniforms(
                (0.0, 0.0),
                (1.0, 1.0),
                (warm_w as f32, warm_h as f32),
                (warm_w as f32, warm_h as f32),
                0.0,
                0.0,
                0.0,
                0.0,
                [0.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0, 1.0],
                0.0,
                (-1.0, -1.0),
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                false,
                1.0,
                (0.5, 0.5),
                0.0,
            );

            let _ = compositor.render_frame(&uniforms);
            println!(
                "[Export][Warmup] GPU export pipeline warmed up in {:.2}s",
                warmup_start.elapsed().as_secs_f64()
            );
        }
        Err(err) => {
            eprintln!("[Export][Warmup] GPU warm-up failed: {}", err);
        }
    }
}

fn default_opacity() -> f64 {
    1.0
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoSegment {
    pub crop: Option<CropRect>,
    #[serde(default, rename = "trimSegments")]
    pub trim_segments: Vec<TrimSegment>,
    #[serde(default, rename = "textSegments")]
    pub _text_segments: Vec<TextSegment>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrimSegment {
    pub start_time: f64,
    pub end_time: f64,
}

// TextSegment: only needed for serde compat — rendering uses BakedTextOverlay
#[derive(Deserialize, Debug, Clone)]
pub struct TextSegment {
    #[serde(flatten)]
    _rest: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BakedTextOverlay {
    pub start_time: f64,
    pub end_time: f64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundConfig {
    pub scale: f64,
    pub border_radius: f64,
    pub background_type: String,
    #[serde(default)]
    pub custom_background: Option<String>,
    pub shadow: f64,
    pub cursor_scale: f64,
    #[serde(default)]
    pub cursor_shadow: f64,
    #[serde(default)]
    pub motion_blur_cursor: f64, // 0-100 intensity
    #[serde(default)]
    pub motion_blur_zoom: f64,
    #[serde(default)]
    pub motion_blur_pan: f64,
}

fn decode_custom_background_bytes(custom_background: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = custom_background.strip_prefix("data:") {
        let (meta, data) = rest
            .split_once(',')
            .ok_or_else(|| "Invalid custom background data URL".to_string())?;
        if !meta.contains(";base64") {
            return Err("Custom background data URL must be base64".to_string());
        }
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("Failed to decode custom background base64: {}", e));
    }

    if let Some(pos) = custom_background.find("/bg-downloaded/") {
        let rel = &custom_background[pos + "/bg-downloaded/".len()..];
        let rel = rel
            .split(['?', '#'])
            .next()
            .unwrap_or(rel);
        if rel.is_empty() || rel.contains("..") || rel.contains('/') || rel.contains('\\') {
            return Err("Invalid downloadable background path".to_string());
        }
        let file_path = dirs::data_local_dir()
            .ok_or_else(|| "Failed to resolve local app data directory".to_string())?
            .join("screen-goated-toolbox")
            .join("backgrounds")
            .join(rel);
        return fs::read(&file_path).map_err(|e| {
            format!(
                "Failed to read downloadable background {}: {}",
                file_path.display(),
                e
            )
        });
    }

    Err("Unsupported custom background source".to_string())
}

fn load_custom_background_rgba(
    custom_background: &str,
    target_w: u32,
    target_h: u32,
) -> Result<Vec<u8>, String> {
    let raw = decode_custom_background_bytes(custom_background)?;
    let decoded = image::load_from_memory(&raw)
        .map_err(|e| format!("Failed to decode custom background image: {}", e))?
        .to_rgba8();

    let src_w = decoded.width().max(1);
    let src_h = decoded.height().max(1);
    let scale = (target_w as f64 / src_w as f64).max(target_h as f64 / src_h as f64);
    let scaled_w = ((src_w as f64 * scale).ceil() as u32).max(target_w);
    let scaled_h = ((src_h as f64 * scale).ceil() as u32).max(target_h);
    let resized = image::imageops::resize(
        &decoded,
        scaled_w,
        scaled_h,
        image::imageops::FilterType::Triangle,
    );
    let crop_x = (scaled_w.saturating_sub(target_w)) / 2;
    let crop_y = (scaled_h.saturating_sub(target_h)) / 2;
    let cropped =
        image::imageops::crop_imm(&resized, crop_x, crop_y, target_w, target_h).to_image();
    Ok(cropped.into_raw())
}

// --- TEXT RENDERER (baked bitmap compositing) ---
// Text overlays are pre-rendered on the JS canvas (identical to preview).
// Rust only alpha-composites the baked bitmaps with per-frame fade applied.

fn composite_baked_text(
    buffer: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    overlay: &BakedTextOverlay,
    fade_alpha: f64,
) {
    if fade_alpha <= 0.001 || overlay.data.is_empty() {
        return;
    }

    let ow = overlay.width as usize;
    let oh = overlay.height as usize;
    let expected = ow * oh * 4;
    if overlay.data.len() < expected {
        return;
    }

    for row in 0..oh {
        let dst_y = overlay.y + row as i32;
        if dst_y < 0 || dst_y >= buf_h as i32 {
            continue;
        }

        for col in 0..ow {
            let dst_x = overlay.x + col as i32;
            if dst_x < 0 || dst_x >= buf_w as i32 {
                continue;
            }

            let src_off = (row * ow + col) * 4;
            let src_a_raw = overlay.data[src_off + 3] as f64 / 255.0;
            let src_a = src_a_raw * fade_alpha;
            if src_a < 0.004 {
                continue;
            } // ~1/255

            let src_r = overlay.data[src_off] as f64;
            let src_g = overlay.data[src_off + 1] as f64;
            let src_b = overlay.data[src_off + 2] as f64;

            let dst_off = (dst_y as usize * buf_w as usize + dst_x as usize) * 4;
            let dst_r = buffer[dst_off] as f64;
            let dst_g = buffer[dst_off + 1] as f64;
            let dst_b = buffer[dst_off + 2] as f64;
            let inv = 1.0 - src_a;

            buffer[dst_off] = (src_r * src_a + dst_r * inv) as u8;
            buffer[dst_off + 1] = (src_g * src_a + dst_g * inv) as u8;
            buffer[dst_off + 2] = (src_b * src_a + dst_b * inv) as u8;
        }
    }
}

// --- GRADIENT COLORS ---

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn hex_to_linear(r: u8, g: u8, b: u8) -> [f32; 4] {
    [
        srgb_to_linear(r as f32 / 255.0),
        srgb_to_linear(g as f32 / 255.0),
        srgb_to_linear(b as f32 / 255.0),
        1.0,
    ]
}

fn get_gradient_colors(bg_type: &str) -> ([f32; 4], [f32; 4]) {
    match bg_type {
        "gradient1" => (
            hex_to_linear(0x4F, 0x7F, 0xD9),
            hex_to_linear(0x8A, 0x72, 0xD8),
        ),
        "gradient2" => (
            hex_to_linear(0xFB, 0x71, 0x85),
            hex_to_linear(0xFD, 0xBA, 0x74),
        ),
        "gradient3" => (
            hex_to_linear(0x10, 0xB9, 0x81),
            hex_to_linear(0x2D, 0xD4, 0xBF),
        ),
        "gradient4" => (
            hex_to_linear(0x06, 0x1A, 0x40),
            hex_to_linear(0xF9, 0x73, 0x16),
        ),
        "gradient5" => (
            hex_to_linear(0x0D, 0x1B, 0x4C),
            hex_to_linear(0xEF, 0x47, 0x6F),
        ),
        "gradient6" => (
            hex_to_linear(0x00, 0xD4, 0xFF),
            hex_to_linear(0xFF, 0x3D, 0x81),
        ),
        "gradient7" => (
            hex_to_linear(0x3F, 0xA7, 0xD6),
            hex_to_linear(0xF2, 0x9E, 0x6D),
        ),
        "white" => (
            hex_to_linear(0xF5, 0xF5, 0xF5),
            hex_to_linear(0xFF, 0xFF, 0xFF),
        ),
        _ => (
            hex_to_linear(0x0A, 0x0A, 0x0A),
            hex_to_linear(0x00, 0x00, 0x00),
        ),
    }
}

// --- NEW SAMPLING LOGIC using Baked Path ---

/// Map compact output time → source time using trim segments.
/// Baked paths now use source time keys, so the export frame loop needs this mapping.
fn output_to_source_time(output_time: f64, trim_segments: &[TrimSegment], trim_start: f64) -> f64 {
    if trim_segments.is_empty() {
        return trim_start + output_time;
    }
    let mut remaining = output_time;
    for seg in trim_segments {
        let seg_len = seg.end_time - seg.start_time;
        if remaining <= seg_len + 1e-9 {
            return seg.start_time + remaining.min(seg_len);
        }
        remaining -= seg_len;
    }
    trim_segments
        .last()
        .map(|s| s.end_time)
        .unwrap_or(output_time)
}

fn sample_baked_path(time: f64, baked_path: &[BakedCameraFrame]) -> (f64, f64, f64) {
    if baked_path.is_empty() {
        return (0.0, 0.0, 1.0);
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return (p.x, p.y, p.zoom);
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return (p.x, p.y, p.zoom);
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let zoom = p1.zoom + (p2.zoom - p1.zoom) * t;

    (x, y, zoom)
}

fn sample_parsed_baked_cursor(
    time: f64,
    baked_path: &[ParsedBakedCursorFrame],
) -> Option<(f64, f64, f64, f32, f64, f64)> {
    if baked_path.is_empty() {
        return None;
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return Some((p.x, p.y, p.scale, p.type_id, p.opacity, p.rotation));
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return Some((p.x, p.y, p.scale, p.type_id, p.opacity, p.rotation));
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let scale = p1.scale + (p2.scale - p1.scale) * t;
    let opacity = p1.opacity + (p2.opacity - p1.opacity) * t;
    let rotation = lerp_angle_rad(p1.rotation, p2.rotation, t);
    let type_id = if t < 0.5 { p1.type_id } else { p2.type_id };

    Some((x, y, scale, type_id, opacity, rotation))
}

fn normalize_angle_rad(a: f64) -> f64 {
    let mut angle = a;
    while angle > std::f64::consts::PI {
        angle -= std::f64::consts::PI * 2.0;
    }
    while angle < -std::f64::consts::PI {
        angle += std::f64::consts::PI * 2.0;
    }
    angle
}

fn lerp_angle_rad(from: f64, to: f64, t: f64) -> f64 {
    let delta = normalize_angle_rad(to - from);
    normalize_angle_rad(from + delta * t)
}

fn cursor_type_to_id(c_type: &str) -> f32 {
    match c_type {
        // ScreenStudio set
        "default-screenstudio" | "default" => 0.0,
        "text-screenstudio" | "text" => 1.0,
        "pointer-screenstudio" | "pointer" => 2.0,
        "openhand-screenstudio" => 3.0,
        "closehand-screenstudio" => 4.0,
        "wait-screenstudio" | "wait" => 5.0,
        "appstarting-screenstudio" | "appstarting" => 6.0,
        "crosshair-screenstudio" | "crosshair" | "cross" => 7.0,
        "resize-ns-screenstudio" | "resize_ns" | "sizens" => 8.0,
        "resize-we-screenstudio" | "resize_we" | "sizewe" => 9.0,
        "resize-nwse-screenstudio" | "resize_nwse" | "sizenwse" => 10.0,
        "resize-nesw-screenstudio" | "resize_nesw" | "sizenesw" => 11.0,

        // macos26 expanded
        "default-macos26" => 12.0,
        "text-macos26" => 13.0,
        "pointer-macos26" => 14.0,
        "openhand-macos26" | "openhand" | "move" | "sizeall" => 15.0,
        "closehand-macos26" | "grabbing" => 16.0,
        "wait-macos26" => 17.0,
        "appstarting-macos26" => 18.0,
        "crosshair-macos26" => 19.0,
        "resize-ns-macos26" => 20.0,
        "resize-we-macos26" => 21.0,
        "resize-nwse-macos26" => 22.0,
        "resize-nesw-macos26" => 23.0,
        "default-sgtcute" => 24.0,
        "text-sgtcute" => 25.0,
        "pointer-sgtcute" => 26.0,
        "openhand-sgtcute" => 27.0,
        "closehand-sgtcute" => 28.0,
        "wait-sgtcute" => 29.0,
        "appstarting-sgtcute" => 30.0,
        "crosshair-sgtcute" => 31.0,
        "resize-ns-sgtcute" => 32.0,
        "resize-we-sgtcute" => 33.0,
        "resize-nwse-sgtcute" => 34.0,
        "resize-nesw-sgtcute" => 35.0,
        "default-sgtcool" => 36.0,
        "text-sgtcool" => 37.0,
        "pointer-sgtcool" => 38.0,
        "openhand-sgtcool" => 39.0,
        "closehand-sgtcool" => 40.0,
        "wait-sgtcool" => 41.0,
        "appstarting-sgtcool" => 42.0,
        "crosshair-sgtcool" => 43.0,
        "resize-ns-sgtcool" => 44.0,
        "resize-we-sgtcool" => 45.0,
        "resize-nwse-sgtcool" => 46.0,
        "resize-nesw-sgtcool" => 47.0,
        "default-sgtai" => 48.0,
        "text-sgtai" => 49.0,
        "pointer-sgtai" => 50.0,
        "openhand-sgtai" => 51.0,
        "closehand-sgtai" => 52.0,
        "wait-sgtai" => 53.0,
        "appstarting-sgtai" => 54.0,
        "crosshair-sgtai" => 55.0,
        "resize-ns-sgtai" => 56.0,
        "resize-we-sgtai" => 57.0,
        "resize-nwse-sgtai" => 58.0,
        "resize-nesw-sgtai" => 59.0,
        "default-sgtpixel" => 60.0,
        "text-sgtpixel" => 61.0,
        "pointer-sgtpixel" => 62.0,
        "openhand-sgtpixel" => 63.0,
        "closehand-sgtpixel" => 64.0,
        "wait-sgtpixel" => 65.0,
        "appstarting-sgtpixel" => 66.0,
        "crosshair-sgtpixel" => 67.0,
        "resize-ns-sgtpixel" => 68.0,
        "resize-we-sgtpixel" => 69.0,
        "resize-nwse-sgtpixel" => 70.0,
        "resize-nesw-sgtpixel" => 71.0,
        "default-jepriwin11" => 72.0,
        "text-jepriwin11" => 73.0,
        "pointer-jepriwin11" => 74.0,
        "openhand-jepriwin11" => 75.0,
        "closehand-jepriwin11" => 76.0,
        "wait-jepriwin11" => 77.0,
        "appstarting-jepriwin11" => 78.0,
        "crosshair-jepriwin11" => 79.0,
        "resize-ns-jepriwin11" => 80.0,
        "resize-we-jepriwin11" => 81.0,
        "resize-nwse-jepriwin11" => 82.0,
        "resize-nesw-jepriwin11" => 83.0,
        "other" => 12.0,
        _ => 0.0,
    }
}

fn collect_used_cursor_slots(baked_cursor: &[BakedCursorFrame]) -> Vec<u32> {
    let mut seen = [false; 84];
    let mut slots = Vec::new();
    for frame in baked_cursor {
        let slot = cursor_type_to_id(&frame.cursor_type) as u32;
        let idx = slot as usize;
        if idx < seen.len() && !seen[idx] {
            seen[idx] = true;
            slots.push(slot);
        }
    }
    if slots.is_empty() {
        slots.push(0);
    }
    slots
}

fn parse_baked_cursor_frames(baked_cursor: &[BakedCursorFrame]) -> Vec<ParsedBakedCursorFrame> {
    baked_cursor
        .iter()
        .map(|frame| ParsedBakedCursorFrame {
            // Keep JSON schema compatibility for click-state payload even if export path doesn't use it.
            // Reading the field here also prevents dead-code warnings.
            time: {
                let _ = frame.is_clicked;
                frame.time
            },
            x: frame.x,
            y: frame.y,
            scale: frame.scale,
            type_id: cursor_type_to_id(frame.cursor_type.as_str()),
            opacity: frame.opacity,
            rotation: frame.rotation,
        })
        .collect()
}

pub fn get_default_export_dir() -> String {
    dirs::download_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FileOpenDialog, IFileOpenDialog, IShellItem,
        SHCreateItemFromParsingName, SHGetKnownFolderPath, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
        FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|e| e.to_string())?;

        let _ = dialog.SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);

        if let Some(dir) = initial_dir.filter(|d| !d.trim().is_empty()) {
            let dir_w: Vec<u16> = std::ffi::OsStr::new(&dir)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(dir_w.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
        } else if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
        {
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
        }

        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }

        let result = dialog.GetResult().map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();

        if path_str.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path_str))
        }
    }
}

fn format_trim_select_expr(trim_segments: &[TrimSegment]) -> String {
    if trim_segments.is_empty() {
        return "1".to_string();
    }
    trim_segments
        .iter()
        .map(|s| format!("between(t,{:.6},{:.6})", s.start_time, s.end_time))
        .collect::<Vec<_>>()
        .join("+")
}

fn ffmpeg_has_nvenc(ffmpeg_path: &std::path::Path) -> bool {
    *NVENC_AVAILABLE.get_or_init(|| {
        match Command::new(ffmpeg_path)
            .args(["-hide_banner", "-encoders"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains("h264_nvenc"),
            Err(_) => false,
        }
    })
}

fn source_dim_cache_key(source_video_path: &str) -> String {
    let mut key = source_video_path.to_string();
    if let Ok(meta) = fs::metadata(source_video_path) {
        let len = meta.len();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .unwrap_or_default();
        key.push('|');
        key.push_str(&len.to_string());
        key.push('|');
        key.push_str(&modified.as_secs().to_string());
        key.push('|');
        key.push_str(&modified.subsec_nanos().to_string());
    } else {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        key.push('|');
        key.push_str(&now.as_secs().to_string());
    }
    key
}

fn probe_source_dimensions(
    ffprobe_path: &std::path::Path,
    source_video_path: &str,
) -> Result<(u32, u32, bool), String> {
    let key = source_dim_cache_key(source_video_path);
    let cache = SOURCE_DIM_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((w, h)) = cache.lock().unwrap().get(&key).copied() {
        return Ok((w, h, true));
    }

    let probe = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            source_video_path,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Probe failed: {}", e))?;

    let dim_str = String::from_utf8_lossy(&probe.stdout);
    let dims: Vec<&str> = dim_str.trim().split('x').collect();
    let src_w: u32 = dims.first().and_then(|s| s.parse().ok()).unwrap_or(1920);
    let src_h: u32 = dims.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);

    cache.lock().unwrap().insert(key, (src_w, src_h));
    Ok((src_w, src_h, false))
}

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let export_total_start = Instant::now();
    let _active_export_guard = ExportActiveGuard::activate();
    EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    let parse_start = Instant::now();
    let mut config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let parse_secs = parse_start.elapsed().as_secs_f64();

    println!("[Export] Starting GPU-accelerated export...");

    let baked_path = config.baked_path.unwrap_or_default();
    let baked_cursor = config.baked_cursor_path.unwrap_or_default();
    let parsed_baked_cursor = parse_baked_cursor_frames(&baked_cursor);
    let used_cursor_slots = collect_used_cursor_slots(&baked_cursor);

    // 0. Handle Source Video/Audio
    let mut temp_video_path: Option<PathBuf> = None;
    let mut temp_audio_path: Option<PathBuf> = None;

    let source_video_path = if let Some(video_data) = config.video_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source.mp4");
        fs::write(&path, video_data).map_err(|e| format!("Failed to write temp video: {}", e))?;
        temp_video_path = Some(path.clone());
        path.to_string_lossy().to_string()
    } else {
        VIDEO_PATH
            .lock()
            .unwrap()
            .clone()
            .ok_or("No source video found")?
    };

    let source_audio_path = if let Some(audio_data) = config.audio_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source_audio.wav");
        fs::write(&path, audio_data).map_err(|e| format!("Failed to write temp audio: {}", e))?;
        temp_audio_path = Some(path.clone());
        Some(path.to_string_lossy().to_string())
    } else if !config.audio_path.is_empty() {
        Some(config.audio_path.clone())
    } else {
        None
    };

    let output_base_dir = if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    };

    fs::create_dir_all(&output_base_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    let output_path = output_base_dir.join(format!(
        "SGT_Export_{}.mp4",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));

    // 1. Setup FFmpeg
    let ffmpeg_path = super::get_ffmpeg_path();
    let ffprobe_path = super::get_ffprobe_path();

    if !ffmpeg_path.exists() {
        return Err("FFmpeg not found.".to_string());
    }

    // 2. Probe source dimensions
    let (src_w, src_h) = if config.source_width > 0 && config.source_height > 0 {
        (config.source_width, config.source_height)
    } else {
        let (w, h, _) = probe_source_dimensions(&ffprobe_path, &source_video_path)?;
        (w, h)
    };

    // 3. Calculate dimensions
    let crop = &config.segment.crop;
    let crop_w = if let Some(c) = crop {
        (src_w as f64 * c.width) as u32
    } else {
        src_w
    };
    let crop_h = if let Some(c) = crop {
        (src_h as f64 * c.height) as u32
    } else {
        src_h
    };

    let crop_x_offset = if let Some(c) = crop {
        src_w as f64 * c.x
    } else {
        0.0
    };
    let crop_y_offset = if let Some(c) = crop {
        src_h as f64 * c.y
    } else {
        0.0
    };

    let out_w = if config.width == 0 {
        crop_w
    } else {
        config.width
    };
    let out_h = if config.height == 0 {
        crop_h
    } else {
        config.height
    };
    // Ensure even dimensions
    let out_w = out_w - (out_w % 2);
    let out_h = out_h - (out_h % 2);

    let scale_factor = config.background_config.scale / 100.0;
    let crop_aspect = crop_w as f64 / crop_h as f64;
    let out_aspect = out_w as f64 / out_h as f64;

    let (video_w, video_h) = if crop_aspect > out_aspect {
        let w = (out_w as f64 * scale_factor) as u32;
        let h = (w as f64 / crop_aspect) as u32;
        (w & !1, h & !1)
    } else {
        let h = (out_h as f64 * scale_factor) as u32;
        let w = (h as f64 * crop_aspect) as u32;
        (w & !1, h & !1)
    };

    // 4. Initialize GPU compositor with cursor
    // Custom backgrounds can be supersampled up to max zoom (capped) so high zooms stay crisp.
    let mut background_w = out_w;
    let mut background_h = out_h;
    if config.background_config.background_type == "custom" {
        let max_zoom = baked_path
            .iter()
            .fold(1.0_f64, |acc, p| acc.max(p.zoom.max(1.0)));
        let desired_scale = max_zoom.ceil().clamp(1.0, 2.0);
        let cap_scale = (8192.0 / out_w as f64).min(8192.0 / out_h as f64).max(1.0);
        let bg_scale = desired_scale.min(cap_scale);
        background_w = ((out_w as f64 * bg_scale).round() as u32).max(out_w);
        background_h = ((out_h as f64 * bg_scale).round() as u32).max(out_h);
    }

    let gpu_init_start = Instant::now();
    let compositor = GpuCompositor::new(out_w, out_h, crop_w, crop_h, background_w, background_h)
        .map_err(|e| format!("GPU init failed: {}", e))?;
    let gpu_device_secs = gpu_init_start.elapsed().as_secs_f64();

    let cursor_init_start = Instant::now();
    let used_cached_atlas = compositor.init_cursor_texture_fast(&used_cursor_slots);
    let cursor_init_secs = cursor_init_start.elapsed().as_secs_f64();
    let cursor_init_mode = if used_cached_atlas {
        "cached-full"
    } else {
        "fast-partial"
    };
    let mut use_custom_background = false;
    if config.background_config.background_type == "custom" {
        if let Some(custom_background) = &config.background_config.custom_background {
            match load_custom_background_rgba(custom_background, background_w, background_h) {
                Ok(rgba) => {
                    compositor.upload_background(&rgba);
                    use_custom_background = true;
                }
                Err(e) => return Err(format!("Custom background load failed: {}", e)),
            }
        } else {
            return Err("Custom background is selected but missing path".to_string());
        }
    }

    // 5. Start FFmpeg decoder
    let decoder_start = Instant::now();
    let crop_filter = if let Some(c) = crop {
        let crop_x = (src_w as f64 * c.x) as u32;
        let crop_y = (src_h as f64 * c.y) as u32;
        format!("crop={}:{}:{}:{}", crop_w, crop_h, crop_x, crop_y)
    } else {
        "null".to_string()
    };

    // Decoder must output at (framerate / speed) fps so each decoded frame
    // matches one iteration of the frame loop which advances source time by
    // dt * speed per step. E.g. 24fps @ 2x speed → decoder at 12fps.
    let decoder_fps = config.framerate as f64 / config.speed;
    let decoder_fps_str = format!("{:.4}", decoder_fps);
    let has_trim_segments = !config.segment.trim_segments.is_empty();
    let select_expr = format_trim_select_expr(&config.segment.trim_segments);
    let select_filter = format!("select='{}',setpts=N/FRAME_RATE/TB", select_expr);
    let decoder_filter = if has_trim_segments {
        format!("{},{}", select_filter, crop_filter)
    } else {
        crop_filter.clone()
    };

    let mut decoder_cmd = Command::new(&ffmpeg_path);
    decoder_cmd.args(["-hide_banner", "-loglevel", "error"]);
    if has_trim_segments {
        decoder_cmd.args([
            "-i",
            &source_video_path,
            "-vf",
            &decoder_filter,
            "-r",
            &decoder_fps_str,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", crop_w, crop_h),
            "-",
        ]);
    } else {
        decoder_cmd.args([
            "-ss",
            &config.trim_start.to_string(),
            "-t",
            &config.duration.to_string(),
            "-i",
            &source_video_path,
            "-vf",
            &decoder_filter,
            "-r",
            &decoder_fps_str,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", crop_w, crop_h),
            "-",
        ]);
    }

    let mut decoder = decoder_cmd
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Decoder failed: {}", e))?;
    let decoder_start_secs = decoder_start.elapsed().as_secs_f64();

    let mut decoder_stdout = decoder
        .stdout
        .take()
        .ok_or("Failed to open decoder stdout")?;

    // 6. Start FFmpeg encoder
    let encoder_start = Instant::now();

    // CRF 18: high quality, optimal for screen recordings with sharp text
    let crf = "18";

    let has_audio = source_audio_path.is_some();
    let mut encoder_args_base = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", out_w, out_h),
        "-r".to_string(),
        config.framerate.to_string(),
        "-i".to_string(),
        "-".to_string(),
    ];

    if let Some(audio) = &source_audio_path {
        let mut audio_filter = if has_trim_segments {
            format!("aselect='{}',asetpts=N/SR/TB", select_expr)
        } else {
            "anull".to_string()
        };
        if config.speed != 1.0 {
            audio_filter = format!("{},atempo={}", audio_filter, config.speed.clamp(0.5, 2.0));
        }

        if has_trim_segments {
            encoder_args_base.extend([
                "-i".to_string(),
                audio.clone(),
                "-af".to_string(),
                audio_filter,
            ]);
        } else {
            encoder_args_base.extend([
                "-ss".to_string(),
                config.trim_start.to_string(),
                "-t".to_string(),
                config.duration.to_string(),
                "-i".to_string(),
                audio.clone(),
                "-af".to_string(),
                audio_filter,
            ]);
        }
    }

    let x264_preset = "veryfast";
    let can_use_nvenc = ffmpeg_has_nvenc(&ffmpeg_path);
    let is_custom_background = config.background_config.background_type == "custom";

    let make_encoder_args = |use_nvenc: bool| {
        let mut args = encoder_args_base.clone();
        if use_nvenc {
            if is_custom_background {
                // Detailed moving image backgrounds can bottleneck NVENC at high quality.
                // Use a throughput-oriented profile to keep export latency predictable.
                args.extend([
                    "-c:v".to_string(),
                    "h264_nvenc".to_string(),
                    "-preset".to_string(),
                    "p1".to_string(), // fastest
                    "-tune".to_string(),
                    "ll".to_string(),
                    "-rc".to_string(),
                    "constqp".to_string(),
                    "-qp".to_string(),
                    "28".to_string(),
                    "-bf".to_string(),
                    "0".to_string(),
                    "-rc-lookahead".to_string(),
                    "0".to_string(),
                    "-spatial-aq".to_string(),
                    "0".to_string(),
                    "-temporal-aq".to_string(),
                    "0".to_string(),
                    "-b_ref_mode".to_string(),
                    "disabled".to_string(),
                    "-pix_fmt".to_string(),
                    "yuv420p".to_string(),
                ]);
            } else {
                args.extend([
                    "-c:v".to_string(),
                    "h264_nvenc".to_string(),
                    "-preset".to_string(),
                    "p1".to_string(), // fastest
                    "-cq".to_string(),
                    "19".to_string(),
                    "-b:v".to_string(),
                    "0".to_string(),
                    "-pix_fmt".to_string(),
                    "yuv420p".to_string(),
                ]);
            }
        } else {
            args.extend([
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                x264_preset.to_string(),
                "-crf".to_string(),
                crf.to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
            ]);
        }

        if has_audio {
            args.extend([
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
            ]);
        }

        if !is_custom_background {
            args.extend(["-movflags".to_string(), "+faststart".to_string()]);
        }
        args.push(output_path.to_str().unwrap().to_string());
        args
    };

    let mut encoder_args = make_encoder_args(can_use_nvenc);
    let mut encoder_name = if can_use_nvenc {
        "h264_nvenc".to_string()
    } else {
        format!("libx264({})", x264_preset)
    };
    let mut encoder = match Command::new(&ffmpeg_path)
        .args(&encoder_args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(enc) => enc,
        Err(first_err) => {
            if can_use_nvenc {
                let _ = first_err;
                encoder_args = make_encoder_args(false);
                encoder_name = format!("libx264({})", x264_preset);
                Command::new(&ffmpeg_path)
                    .args(&encoder_args)
                    .creation_flags(CREATE_NO_WINDOW)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|e| format!("Encoder failed (nvenc fallback): {}", e))?
            } else {
                return Err(format!("Encoder failed: {}", first_err));
            }
        }
    };
    let encoder_start_secs = encoder_start.elapsed().as_secs_f64();

    let mut encoder_stdin = encoder.stdin.take().ok_or("Failed to open encoder stdin")?;
    let (frame_tx, frame_rx) = sync_channel::<Vec<u8>>(8);
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(8);
    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        while let Ok(mut frame) = frame_rx.recv() {
            if let Err(e) = encoder_stdin.write_all(&frame) {
                return Err(format!("Encoder stdin write failed: {}", e));
            }
            frame.clear();
            // Never block writer shutdown on recycle path.
            let _ = recycle_tx.try_send(frame);
        }
        Ok(())
    });

    // Store PIDs so cancel_export() can kill them to unblock pipe IO
    *EXPORT_PIDS.lock().unwrap() = (decoder.id(), encoder.id());

    // 7. Process frames
    let bg_type = &config.background_config.background_type;
    let (gradient1, gradient2) = get_gradient_colors(bg_type);
    let background_style = if bg_type == "gradient4" {
        1.0
    } else if bg_type == "gradient5" {
        2.0
    } else if bg_type == "gradient6" {
        3.0
    } else if bg_type == "gradient7" {
        4.0
    } else {
        0.0
    };
    let frame_size = (crop_w * crop_h * 4) as usize;
    let mut buffer = vec![0u8; frame_size];

    // Use a counted loop that matches the decoder's frame count exactly.
    // The decoder outputs at decoder_fps for config.duration seconds.
    // Using round() to match ffmpeg's internal rounding of -r * -t.
    let total_frames = ((config.duration * decoder_fps).round() as u32).max(1);
    let step = config.speed / config.framerate as f64; // source-time advance per output frame
    let mut frame_count = 0u32;
    let mut cancelled = false;
    let export_start = Instant::now();
    let mut first_frame_secs: Option<f64> = None;
    let frame_stage_start = Instant::now();
    let mut rendered = Vec::with_capacity((out_w * out_h * 4) as usize);
    let mut writer_error: Option<String> = None;

    // --- Motion blur configuration (slider: 0=off, 50=standard 180°, 100=heavy 360°) ---
    let blur_cursor_val = config.background_config.motion_blur_cursor;
    let blur_zoom_val = config.background_config.motion_blur_zoom;
    let blur_pan_val = config.background_config.motion_blur_pan;
    let max_blur_val = blur_cursor_val.max(blur_zoom_val).max(blur_pan_val);
    let any_blur = max_blur_val > 0.0;
    // Shutter angle: 50→270° (cinematic+), 100→540° (extreme)
    let shutter_angle_deg = max_blur_val * 5.4;
    let shutter_fraction = shutter_angle_deg / 360.0;
    // Sample count for smooth blur in export (GPU accumulation = cheap per-sample)
    let blur_samples: usize = if shutter_angle_deg <= 0.0 {
        1
    } else if shutter_angle_deg <= 90.0 {
        12
    } else if shutter_angle_deg <= 180.0 {
        16
    } else if shutter_angle_deg <= 360.0 {
        24
    } else {
        32
    };

    // Pre-compute constants for uniform builder (all Copy — no borrow issues)
    // Match preview model:
    // - fixed 0.5 shadow alpha when enabled
    // - blur directly from slider
    // - vertical offset = 0.5 * slider
    let shadow_opacity_f32 = if config.background_config.shadow > 0.0 { 0.5 } else { 0.0 };
    let shadow_blur_f32 = config.background_config.shadow as f32;
    let border_radius_f32 = config.background_config.border_radius as f32;
    let shadow_offset_f32 = config.background_config.shadow as f32 * 0.5;
    let cursor_scale_cfg = config.background_config.cursor_scale;
    let cursor_shadow_f32 = (config.background_config.cursor_shadow as f32 / 100.0).clamp(0.0, 2.0);
    let size_ratio_cursor = (out_w as f64 / crop_w as f64).min(out_h as f64 / crop_h as f64);
    let video_w_f = video_w as f64;
    let video_h_f = video_h as f64;
    let out_w_f = out_w as f64;
    let out_h_f = out_h as f64;
    let crop_w_f = crop_w as f64;
    let crop_h_f = crop_h as f64;
    let out_w_f32 = out_w as f32;
    let out_h_f32 = out_h as f32;

    let build_uniforms = |cam_x_raw: f64,
                          cam_y_raw: f64,
                          zoom: f64,
                          cursor: Option<(f64, f64, f64, f32, f64, f64)>,
                          t: f64|
     -> CompositorUniforms {
        let cam_x = cam_x_raw - crop_x_offset;
        let cam_y = cam_y_raw - crop_y_offset;
        let zvw = video_w_f * zoom;
        let zvh = video_h_f * zoom;
        let rx = (cam_x / crop_w_f).clamp(0.0, 1.0);
        let ry = (cam_y / crop_h_f).clamp(0.0, 1.0);
        let zsx = (1.0 - zoom) * rx;
        let zsy = (1.0 - zoom) * ry;
        let bcx = (1.0 - video_w_f / out_w_f) / 2.0 * zoom;
        let bcy = (1.0 - video_h_f / out_h_f) / 2.0 * zoom;
        let ox = zsx + bcx;
        let oy = zsy + bcy;

        let (cp_x, cp_y, cs, co, ct, cr) = if let Some((cx, cy, c_s, c_t, c_o, c_r)) = cursor {
            if c_o < 0.001 {
                (-100.0_f32, -100.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32)
            } else {
                let rel_x = (cx - crop_x_offset) / crop_w_f;
                let rel_y = (cy - crop_y_offset) / crop_h_f;
                let fs = c_s * cursor_scale_cfg * zoom * size_ratio_cursor;
                (
                    rel_x as f32,
                    rel_y as f32,
                    fs as f32,
                    c_o as f32,
                    c_t,
                    c_r as f32,
                )
            }
        } else {
            (-1.0, -1.0, 0.0, 0.0, 0.0, 0.0)
        };

        create_uniforms(
            (ox as f32, oy as f32),
            (zvw as f32 / out_w_f32, zvh as f32 / out_h_f32),
            (out_w_f32, out_h_f32),
            (zvw as f32, zvh as f32),
            border_radius_f32,
            shadow_offset_f32,
            shadow_blur_f32,
            shadow_opacity_f32,
            gradient1,
            gradient2,
            t as f32,
            (cp_x, cp_y),
            cs,
            co,
            ct,
            cr,
            cursor_shadow_f32,
            use_custom_background,
            zoom as f32,
            (rx as f32, ry as f32),
            background_style,
        )
    };

    println!(
        "[Export] Frame loop: total_frames={}, decoder_fps={:.2}, step={:.6}, blur={}",
        total_frames, decoder_fps, step, any_blur
    );

    for frame_idx in 0..total_frames {
        if EXPORT_CANCELLED.load(Ordering::SeqCst) {
            println!(
                "[Cancel] Flag detected at frame {}, breaking loop",
                frame_idx
            );
            cancelled = true;
            break;
        }
        if std::io::Read::read_exact(&mut decoder_stdout, &mut buffer).is_err() {
            println!(
                "[Export] read_exact failed at frame {}/{}",
                frame_idx, total_frames
            );
            break;
        }

        let current_time = frame_idx as f64 * step;
        // Map compact output time → source time for baked path lookups.
        // Baked paths use source-time keys so camera/cursor evolve naturally
        // through hidden trim gaps (no jarring jumps at segment bridges).
        let source_time = output_to_source_time(
            current_time,
            &config.segment.trim_segments,
            config.trim_start,
        );
        compositor.upload_frame(&buffer);

        // Sample center state (always needed as fallback / single-render)
        let (center_cam_x, center_cam_y, center_zoom) = sample_baked_path(source_time, &baked_path);
        let center_cursor = sample_parsed_baked_cursor(source_time, &parsed_baked_cursor);

        if any_blur {
            let half_shutter = step * shutter_fraction / 2.0;
            let t_start = source_time - half_shutter;
            let t_end = source_time + half_shutter;

            // Detect motion per enabled channel (higher thresholds to avoid micro-jitter blur)
            let (cam_x0, cam_y0, zoom0) = sample_baked_path(t_start, &baked_path);
            let (cam_x1, cam_y1, zoom1) = sample_baked_path(t_end, &baked_path);
            let zoom_moved = blur_zoom_val > 0.0 && (zoom0 - zoom1).abs() > 1e-5;
            let pan_moved = blur_pan_val > 0.0
                && ((cam_x0 - cam_x1).abs() > 1.0 || (cam_y0 - cam_y1).abs() > 1.0);
            let camera_needs_sample = zoom_moved || pan_moved;

            let cursor0 = sample_parsed_baked_cursor(t_start, &parsed_baked_cursor);
            let cursor1 = sample_parsed_baked_cursor(t_end, &parsed_baked_cursor);
            let cursor_moved = blur_cursor_val > 0.0
                && match (cursor0, cursor1) {
                    (Some(c0), Some(c1)) => (c0.0 - c1.0).abs() > 2.0 || (c0.1 - c1.1).abs() > 2.0,
                    _ => false,
                };

            // Keep one consistent accumulation path to avoid pulsing from mode switches.
            // When there is no motion, use a single sample to preserve shadow contrast.
            let effective_samples = if camera_needs_sample || cursor_moved {
                blur_samples
            } else {
                1
            };
            for si in 0..effective_samples {
                let f = (si as f64 + 0.5) / effective_samples as f64 - 0.5;

                // Per-channel: use sub-time for enabled channels, center for disabled
                let cam_sub_t = source_time + f * (step * shutter_fraction);
                let (sub_raw_x, sub_raw_y, sub_zoom) = if camera_needs_sample {
                    let (rx, ry, rz) = sample_baked_path(cam_sub_t, &baked_path);
                    let sx = if blur_pan_val > 0.0 { rx } else { center_cam_x };
                    let sy = if blur_pan_val > 0.0 { ry } else { center_cam_y };
                    let sz = if blur_zoom_val > 0.0 { rz } else { center_zoom };
                    (sx, sy, sz)
                } else {
                    (center_cam_x, center_cam_y, center_zoom)
                };

                let cursor_sub_t = source_time + f * (step * (blur_cursor_val * 3.6 / 360.0));
                let sub_cursor = if cursor_moved {
                    sample_parsed_baked_cursor(cursor_sub_t, &parsed_baked_cursor)
                } else {
                    center_cursor
                };

                let weight = 1.0 / (si + 1) as f64;
                let sub_uniforms =
                    build_uniforms(sub_raw_x, sub_raw_y, sub_zoom, sub_cursor, cam_sub_t);
                compositor.render_accumulate(&sub_uniforms, si == 0, weight);
            }
            // Single readback after all sub-frames
            compositor.readback_output(&mut rendered);
        } else {
            // No blur enabled — single render
            let uniforms = build_uniforms(
                center_cam_x,
                center_cam_y,
                center_zoom,
                center_cursor,
                source_time,
            );
            compositor.render_frame_into(&uniforms, &mut rendered);
        }

        // --- RENDER TEXT OVERLAY (baked bitmaps) ---
        let fade_dur = 0.3_f64;
        for overlay in &config.baked_text_overlays {
            if current_time >= overlay.start_time && current_time <= overlay.end_time {
                let elapsed = current_time - overlay.start_time;
                let remaining = overlay.end_time - current_time;
                let mut fade = 1.0_f64;
                if elapsed < fade_dur {
                    fade = elapsed / fade_dur;
                }
                if remaining < fade_dur {
                    fade = fade.min(remaining / fade_dur);
                }
                composite_baked_text(&mut rendered, out_w, out_h, overlay, fade);
            }
        }

        let mut to_send = Vec::new();
        std::mem::swap(&mut rendered, &mut to_send);
        if frame_tx.send(to_send).is_err() {
            writer_error = Some("Encoder writer thread stopped".to_string());
            break;
        }
        match recycle_rx.try_recv() {
            Ok(mut recycled) => {
                recycled.clear();
                rendered = recycled;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {}
        }

        frame_count += 1;
        if first_frame_secs.is_none() {
            first_frame_secs = Some(frame_stage_start.elapsed().as_secs_f64());
        }
        if frame_count.is_multiple_of(15) {
            let elapsed = export_start.elapsed().as_secs_f64();
            let pct = (frame_count as f64 / total_frames as f64 * 100.0).min(100.0);
            let eta = if frame_count > 0 {
                (elapsed / frame_count as f64) * (total_frames - frame_count) as f64
            } else {
                0.0
            };
            push_export_progress(pct, eta);
        }
    }

    println!(
        "[Export] Loop exited: frame_count={}, cancelled={}",
        frame_count, cancelled
    );
    let frame_loop_secs = export_start.elapsed().as_secs_f64();

    // Stop writer and close encoder stdin.
    drop(frame_tx);
    if cancelled {
        // If cancelled, kill encoder first to unblock a possible blocked stdin writer.
        let _ = encoder.kill();
    }
    match writer_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            if !cancelled {
                writer_error = Some(e);
            }
        }
        Err(_) => {
            if !cancelled {
                writer_error = Some("Encoder writer thread panicked".to_string());
            }
        }
    }

    // Clear stored PIDs (processes are about to be cleaned up)
    *EXPORT_PIDS.lock().unwrap() = (0, 0);

    // Close decoder pipe and kill decoder (may have unread frames buffered)
    drop(decoder_stdout);
    let _ = decoder.kill();
    let _ = decoder.wait();

    if cancelled {
        // On cancel: encoder already killed before joining writer.
        let _ = encoder.wait();
    }

    // Clean up temp files
    if let Some(p) = temp_video_path {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = temp_audio_path {
        let _ = fs::remove_file(p);
    }

    if cancelled {
        let _ = fs::remove_file(&output_path);
        println!(
            "[Export][Summary] status=cancelled out={}x{} fps={} speed={:.2} dur={:.3}s frames={}/{} slots={} parse={:.3}s gpu={:.3}s cursor={:.3}s({}) dec_spawn={:.3}s enc_spawn={:.3}s first_frame={}s loop={:.3}s total={:.3}s encoder={}",
            out_w,
            out_h,
            config.framerate,
            config.speed,
            config.duration,
            frame_count,
            total_frames,
            used_cursor_slots.len(),
            parse_secs,
            gpu_device_secs,
            cursor_init_secs,
            cursor_init_mode,
            decoder_start_secs,
            encoder_start_secs,
            first_frame_secs.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".to_string()),
            frame_loop_secs,
            export_total_start.elapsed().as_secs_f64(),
            encoder_name
        );
        return Ok(serde_json::json!({ "status": "cancelled" }));
    }

    if let Some(err) = writer_error {
        println!(
            "[Export][Summary] status=error out={}x{} fps={} speed={:.2} dur={:.3}s frames={}/{} slots={} parse={:.3}s gpu={:.3}s cursor={:.3}s({}) dec_spawn={:.3}s enc_spawn={:.3}s first_frame={}s loop={:.3}s total={:.3}s encoder={} error=writer",
            out_w,
            out_h,
            config.framerate,
            config.speed,
            config.duration,
            frame_count,
            total_frames,
            used_cursor_slots.len(),
            parse_secs,
            gpu_device_secs,
            cursor_init_secs,
            cursor_init_mode,
            decoder_start_secs,
            encoder_start_secs,
            first_frame_secs.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".to_string()),
            frame_loop_secs,
            export_total_start.elapsed().as_secs_f64(),
            encoder_name
        );
        return Err(err);
    }

    // Wait for encoder to finish flushing (H.264 B-frames, moov atom, etc.)
    let encoder_result = encoder.wait();
    let total_wall_secs = export_total_start.elapsed().as_secs_f64();

    match encoder_result {
        Ok(status) if status.success() => {
            println!(
                "[Export][Summary] status=success out={}x{} fps={} speed={:.2} dur={:.3}s frames={}/{} slots={} parse={:.3}s gpu={:.3}s cursor={:.3}s({}) dec_spawn={:.3}s enc_spawn={:.3}s first_frame={}s loop={:.3}s total={:.3}s encoder={}",
                out_w,
                out_h,
                config.framerate,
                config.speed,
                config.duration,
                frame_count,
                total_frames,
                used_cursor_slots.len(),
                parse_secs,
                gpu_device_secs,
                cursor_init_secs,
                cursor_init_mode,
                decoder_start_secs,
                encoder_start_secs,
                first_frame_secs.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".to_string()),
                frame_loop_secs,
                total_wall_secs,
                encoder_name
            );
            Ok(serde_json::json!({
                "status": "success",
                "path": output_path.to_string_lossy(),
                "frames": frame_count
            }))
        }
        _ => {
            println!(
                "[Export][Summary] status=error out={}x{} fps={} speed={:.2} dur={:.3}s frames={}/{} slots={} parse={:.3}s gpu={:.3}s cursor={:.3}s({}) dec_spawn={:.3}s enc_spawn={:.3}s first_frame={}s loop={:.3}s total={:.3}s encoder={} error=encoder",
                out_w,
                out_h,
                config.framerate,
                config.speed,
                config.duration,
                frame_count,
                total_frames,
                used_cursor_slots.len(),
                parse_secs,
                gpu_device_secs,
                cursor_init_secs,
                cursor_init_mode,
                decoder_start_secs,
                encoder_start_secs,
                first_frame_secs.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".to_string()),
                frame_loop_secs,
                total_wall_secs,
                encoder_name
            );
            Err("Encoder failed".to_string())
        }
    }
}
