mod camera_path;
mod color;
pub mod config;
mod cursor;
mod cursor_path;
mod overlay;
mod progress;
pub mod sampling;
pub mod staging;
mod util;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use config::{ExportConfig, ExportRuntimeDiagnostics};
use cursor::{collect_used_cursor_slots, parse_baked_cursor_frames};
use overlay::load_custom_background_rgba;
use sampling::{sample_baked_path, sample_parsed_baked_cursor};

use super::gpu_export::{create_uniforms, CompositorUniforms, GpuCompositor};
use super::gpu_pipeline;
use super::mf_decode;
use super::mf_encode;
use super::SR_HWND;
use crate::overlay::screen_record::engine::VIDEO_PATH;

pub use progress::{export_replay_args_path, persist_export_result, push_export_progress};

pub fn prewarm_custom_background(url: &str) -> Result<(), String> {
    overlay::prewarm_custom_background(url)
}

/// Flag to signal export cancellation from the frontend.
static EXPORT_CANCELLED: AtomicBool = AtomicBool::new(false);
/// Ensures GPU export warm-up runs once per app session.
static EXPORT_GPU_WARMED: AtomicBool = AtomicBool::new(false);
/// Indicates an export is actively running.
static EXPORT_ACTIVE: AtomicBool = AtomicBool::new(false);

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
    println!("[Cancel] Cancellation signaled");
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
                0.0, // render_mode
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
                0.0,
                0.0,
            );

            let mut warm_compositor = compositor;
            let _ = warm_compositor.render_frame(&uniforms);
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

pub fn get_default_export_dir() -> String {
    dirs::download_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    util::pick_export_folder(initial_dir)
}

pub fn get_export_capabilities() -> serde_json::Value {
    serde_json::json!({
        "pipeline": "zero_copy_gpu",
        "mf_h264": true,
    })
}

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let export_total_start = Instant::now();
    let _active_export_guard = ExportActiveGuard::activate();
    EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    progress::persist_replay_args(&args);

    let parse_start = Instant::now();
    let mut config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let parse_secs = parse_start.elapsed().as_secs_f64();

    println!("[Export] Starting zero-copy GPU export...");

    // Merge staged baked data (sent via chunked IPC) with inline config data.
    // Staged data takes priority when config arrays are empty.
    let staged = staging::take_staged();

    let mut baked_path = match config.baked_path.take() {
        Some(v) if !v.is_empty() => v,
        _ => staged.camera_frames,
    };
    let mut baked_cursor = match config.baked_cursor_path.take() {
        Some(v) if !v.is_empty() => v,
        _ => staged.cursor_frames,
    };

    // Ensure baked paths are sorted by time (partition_point requires sorted input).
    // Chunked IPC staging may deliver frames out of order.
    let cam_unsorted = baked_path.windows(2).filter(|w| w[1].time < w[0].time).count();
    if cam_unsorted > 0 {
        println!("[Export][WARN] Baked camera path has {} non-monotonic entries — sorting", cam_unsorted);
        baked_path.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    }
    let cur_unsorted = baked_cursor.windows(2).filter(|w| w[1].time < w[0].time).count();
    if cur_unsorted > 0 {
        println!("[Export][WARN] Baked cursor path has {} non-monotonic entries — sorting", cur_unsorted);
        baked_cursor.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    }
    println!(
        "[Export] Baked paths: camera={} frames, cursor={} frames",
        baked_path.len(), baked_cursor.len()
    );
    let overlay_frames = staged.overlay_frames;
    let atlas_rgba = staged.atlas_rgba;
    let atlas_w = staged.atlas_w;
    let atlas_h = staged.atlas_h;

    // 0. Handle Source Video/Audio
    let mut temp_video_path: Option<PathBuf> = None;
    let mut temp_audio_path: Option<PathBuf> = None;

    let explicit_source_video_path = config.source_video_path.trim().to_string();

    let source_video_path = if !explicit_source_video_path.is_empty()
        && Path::new(&explicit_source_video_path).exists()
    {
        explicit_source_video_path
    } else if let Some(video_data) = config.video_data.take() {
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

    // Native zero-copy export writes final MP4 directly (video + optional audio).
    let final_output_path = output_base_dir.join(format!(
        "SGT_Export_{}.mp4",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));

    // Get source dimensions via MF SourceReader (lightweight probe, no GPU)
    mf_decode::mf_startup()?;
    let (src_w, src_h) = if config.source_width > 0 && config.source_height > 0 {
        (config.source_width, config.source_height)
    } else {
        mf_decode::probe_video_dimensions(&source_video_path)?
    };
    println!("[Export] Source dimensions: {}x{}", src_w, src_h);

    // Generate camera path in Rust if not provided by frontend.
    let baked_path = if baked_path.is_empty()
        && (!config.segment.smooth_motion_path.is_empty()
            || !config.segment.zoom_keyframes.is_empty())
    {
        let t0 = Instant::now();
        let generated =
            camera_path::generate_camera_path(&config.segment, src_w, src_h, config.framerate);
        println!(
            "[Export] Camera path: {} frames in {:.3}s",
            generated.len(),
            t0.elapsed().as_secs_f64()
        );
        generated
    } else {
        baked_path
    };

    // Generate cursor path in Rust if not provided by frontend.
    let baked_cursor = if baked_cursor.is_empty() && !config.mouse_positions.is_empty() {
        let t0 = Instant::now();
        let generated = cursor_path::generate_cursor_path(
            &config.segment,
            &config.mouse_positions,
            Some(&config.background_config),
            config.framerate,
        );
        println!(
            "[Export] Cursor path: {} frames in {:.3}s",
            generated.len(),
            t0.elapsed().as_secs_f64()
        );
        generated
    } else {
        baked_cursor
    };

    let parsed_baked_cursor = parse_baked_cursor_frames(&baked_cursor);
    let used_cursor_slots = collect_used_cursor_slots(&baked_cursor);

    // Calculate dimensions
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

    // Initialize GPU compositor — background uploaded at native image size later;
    // object-fit: cover is handled in the shader (no CPU pre-scaling needed).
    let gpu_init_start = Instant::now();
    let mut compositor =
        GpuCompositor::new(out_w, out_h, crop_w, crop_h, out_w, out_h)
            .map_err(|e| format!("GPU init failed: {}", e))?;
    let gpu_device_secs = gpu_init_start.elapsed().as_secs_f64();

    let cursor_init_start = Instant::now();
    compositor.init_cursor_texture_fast(&used_cursor_slots);
    let cursor_init_secs = cursor_init_start.elapsed().as_secs_f64();

    // Upload sprite atlas (text + keystroke overlays pre-rendered by frontend).
    if let Some(rgba) = atlas_rgba {
        compositor.upload_atlas(&rgba, atlas_w, atlas_h);
    }

    let mut use_custom_background = false;
    let mut actual_bg_w = out_w as f32;
    let mut actual_bg_h = out_h as f32;
    if config.background_config.background_type == "custom" {
        if let Some(custom_background) = &config.background_config.custom_background {
            let bg_load_start = Instant::now();
            match load_custom_background_rgba(custom_background) {
                Ok((rgba_arc, tw, th)) => {
                    let bg_load_secs = bg_load_start.elapsed().as_secs_f64();
                    let bg_upload_start = Instant::now();
                    compositor.upload_background(rgba_arc.as_slice(), tw, th);
                    let bg_upload_secs = bg_upload_start.elapsed().as_secs_f64();
                    eprintln!(
                        "[CustomBg] export load {:.3}s + gpu upload {:.3}s ({}x{}, rgba={}B)",
                        bg_load_secs,
                        bg_upload_secs,
                        tw,
                        th,
                        rgba_arc.len()
                    );
                    use_custom_background = true;
                    actual_bg_w = tw as f32;
                    actual_bg_h = th as f32;
                }
                Err(e) => return Err(format!("Custom background load failed: {}", e)),
            }
        } else {
            return Err("Custom background is selected but missing path".to_string());
        }
    }

    // Build zero-copy pipeline config
    let bg = &config.background_config.background_type;
    let (grad1, grad2) = color::get_gradient_colors(bg);
    let bg_style = match bg.as_str() {
        "gradient4" => 1.0,
        "gradient5" => 2.0,
        "gradient6" => 3.0,
        "gradient7" => 4.0,
        _ => 0.0,
    };

    let shadow_opacity = if config.background_config.shadow > 0.0 {
        0.5_f32
    } else {
        0.0
    };
    let shadow_blur = config.background_config.shadow as f32;
    let border_radius = config.background_config.border_radius as f32;
    let shadow_offset = config.background_config.shadow as f32 * 0.5;
    let cursor_scale_cfg = config.background_config.cursor_scale;
    let cursor_shadow =
        (config.background_config.cursor_shadow as f32 / 100.0).clamp(0.0, 2.0);
    let size_ratio = (out_w as f64 / crop_w as f64).min(out_h as f64 / crop_h as f64);
    let vw = video_w as f64;
    let vh = video_h as f64;
    let ow = out_w as f64;
    let oh = out_h as f64;
    let cw = crop_w as f64;
    let ch = crop_h as f64;
    let ow32 = out_w as f32;
    let oh32 = out_h as f32;

    let build_uniforms = |base_time: f64, cam_pan_time: f64, cam_zoom_time: f64, cursor_time: f64| -> CompositorUniforms {
        let (cam_x_raw, cam_y_raw, _) = sample_baked_path(cam_pan_time, &baked_path);
        let (_, _, zoom) = sample_baked_path(cam_zoom_time, &baked_path);
        let cursor_sample = sample_parsed_baked_cursor(cursor_time, &parsed_baked_cursor);

        let cam_x = cam_x_raw - crop_x_offset;
        let cam_y = cam_y_raw - crop_y_offset;
        let zvw = vw * zoom;
        let zvh = vh * zoom;
        let rx = (cam_x / cw).clamp(0.0, 1.0);
        let ry = (cam_y / ch).clamp(0.0, 1.0);
        let zsx = (1.0 - zoom) * rx;
        let zsy = (1.0 - zoom) * ry;
        let bcx = (1.0 - vw / ow) / 2.0 * zoom;
        let bcy = (1.0 - vh / oh) / 2.0 * zoom;
        let ox = zsx + bcx;
        let oy = zsy + bcy;

        let (cp_x, cp_y, cs, co, ct, cr) =
            if let Some((cx, cy, c_s, c_t, c_o, c_r)) = cursor_sample {
                if c_o < 0.001 {
                    (-100.0_f32, -100.0, 0.0, 0.0, 0.0, 0.0)
                } else {
                    let rel_x = (cx - crop_x_offset) / cw;
                    let rel_y = (cy - crop_y_offset) / ch;
                    let fs = c_s * cursor_scale_cfg * zoom * size_ratio;
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
            (zvw as f32 / ow32, zvh as f32 / oh32),
            (ow32, oh32),
            (zvw as f32, zvh as f32),
            border_radius,
            shadow_offset,
            shadow_blur,
            shadow_opacity,
            grad1,
            grad2,
            base_time as f32,
            0.0, // render_mode (0 = all channels)
            (cp_x, cp_y),
            cs,
            co,
            ct,
            cr,
            cursor_shadow,
            use_custom_background,
            zoom as f32,
            (rx as f32, ry as f32),
            bg_style,
            actual_bg_w,
            actual_bg_h,
        )
    };

    let bitrate = if config.target_video_bitrate_kbps > 0 {
        config.target_video_bitrate_kbps
    } else {
        config::compute_default_video_bitrate_kbps(out_w, out_h, config.framerate)
    };

    // Motion blur: derive samples and shutter from config
    let mb_max = config
        .background_config
        .motion_blur_cursor
        .max(config.background_config.motion_blur_zoom)
        .max(config.background_config.motion_blur_pan)
        / 100.0;
    let (mb_samples, mb_shutter) = if mb_max > 0.0001 {
        let samples = (mb_max * 8.0).ceil().clamp(2.0, 8.0) as u32;
        (samples, mb_max.clamp(0.0, 1.0))
    } else {
        (1, 0.0)
    };

    let pipeline_config = gpu_pipeline::PipelineConfig {
        source_video_path: source_video_path.clone(),
        output_path: final_output_path.to_str().unwrap().to_string(),
        audio_path: source_audio_path.clone(),
        output_width: out_w,
        output_height: out_h,
        framerate: config.framerate,
        bitrate_kbps: bitrate,
        speed: config.speed,
        trim_start: config.trim_start,
        duration: config.duration,
        codec: mf_encode::VideoCodec::H264,
        trim_segments: config.segment.trim_segments.clone(),
        motion_blur_samples: mb_samples,
        motion_blur_shutter: mb_shutter,
        blur_zoom:   config.background_config.motion_blur_zoom   > 0.01,
        blur_pan:    config.background_config.motion_blur_pan    > 0.01,
        blur_cursor: config.background_config.motion_blur_cursor > 0.01,
        video_width: crop_w,
        video_height: crop_h,
        crop_x: crop_x_offset as u32,
        crop_y: crop_y_offset as u32,
        overlay_frames,
    };

    let progress_cb: gpu_pipeline::ProgressCallback = Box::new(|pct, eta| {
        push_export_progress(pct, eta);
    });

    println!(
        "[Export] Pipeline config: {}x{} @ {} fps, bitrate={}k, speed={:.2}, trim_start={:.3}, dur={:.3}",
        out_w, out_h, config.framerate, bitrate, config.speed, config.trim_start, config.duration
    );

    let result = gpu_pipeline::run_zero_copy_export(
        &pipeline_config,
        &mut compositor,
        &build_uniforms,
        Some(progress_cb),
        &EXPORT_CANCELLED,
    );

    let _ = mf_decode::mf_shutdown();

    // Clean up temp files
    if let Some(p) = &temp_video_path {
        let _ = fs::remove_file(p);
    }

    match result {
        Ok(r) => {
            let total_secs = export_total_start.elapsed().as_secs_f64();

            if let Some(p) = &temp_audio_path {
                let _ = fs::remove_file(p);
            }

            let output_bytes = fs::metadata(&final_output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let output_duration_sec = (config.duration / config.speed.max(0.1)).max(0.001);
            let actual_total_bitrate_kbps =
                (output_bytes as f64 * 8.0 / output_duration_sec / 1000.0).max(0.0);

            let diagnostics = ExportRuntimeDiagnostics {
                backend: "zero_copy_gpu".to_string(),
                encoder: "mf_h264".to_string(),
                codec: "h264".to_string(),
                turbo: false,
                sfe: false,
                pre_render_policy: config.pre_render_policy.clone(),
                quality_gate_percent: config.quality_gate_percent,
                actual_total_bitrate_kbps,
                expected_total_bitrate_kbps: bitrate as f64,
                bitrate_deviation_percent: if bitrate > 0 {
                    ((actual_total_bitrate_kbps - bitrate as f64).abs() / bitrate as f64) * 100.0
                } else {
                    0.0
                },
            };

            println!(
                "[Export][Summary] status=success out={}x{} fps={} speed={:.2} dur={:.3}s frames={} parse={:.3}s gpu={:.3}s cursor={:.3}s total={:.3}s pipeline=zero_copy actual_kbps={:.1}",
                out_w, out_h, config.framerate, config.speed, config.duration,
                r.frames_encoded, parse_secs, gpu_device_secs, cursor_init_secs,
                total_secs, actual_total_bitrate_kbps
            );

            Ok(serde_json::json!({
                "status": "success",
                "path": final_output_path.to_string_lossy(),
                "frames": r.frames_encoded,
                "bytes": output_bytes,
                "pipeline": "zero_copy_gpu",
                "elapsedSecs": r.elapsed_secs,
                "fps": r.fps,
                "diagnostics": diagnostics
            }))
        }
        Err(e) => {
            if let Some(p) = &temp_audio_path {
                let _ = fs::remove_file(p);
            }
            let _ = fs::remove_file(&final_output_path);

            if EXPORT_CANCELLED.load(Ordering::SeqCst) {
                println!("[Export][Summary] status=cancelled");
                return Ok(serde_json::json!({ "status": "cancelled" }));
            }

            println!("[Export][Summary] status=error error={}", e);
            Err(e)
        }
    }
}
