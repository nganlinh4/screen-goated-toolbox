pub mod anim_cache;
mod audio_mix;
mod background_presets;
mod camera_path;
mod composition;
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
use std::time::{Duration, Instant};

use config::{ExportConfig, ExportRuntimeDiagnostics};
use cursor::{collect_used_cursor_slots, parse_baked_cursor_frames};
use overlay::load_custom_background_rgba;
use sampling::{sample_baked_path, sample_parsed_baked_cursor};
use audio_mix::{build_preprocessed_audio_mix, ExportAudioSource};

use super::SR_HWND;
use super::gpu_export::{
    CompositorUniformParams, CompositorUniforms, GpuCompositor, create_uniforms,
};
use super::gpu_pipeline;
use super::mf_decode;
use super::mf_encode;
use crate::overlay::screen_record::engine::{ENCODER_ACTIVE, IS_RECORDING, VIDEO_PATH};

pub use composition::start_composition_export;
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
const EXPORT_WARMUP_IDLE_DELAY: Duration = Duration::from_secs(15);
const EXPORT_WARMUP_IDLE_POLL: Duration = Duration::from_millis(500);

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

fn sample_capture_dimensions_at_time(
    time: f64,
    mouse_positions: &[config::MousePosition],
    fallback_width: f64,
    fallback_height: f64,
) -> (f64, f64) {
    let mut prev: Option<&config::MousePosition> = None;
    let mut next: Option<&config::MousePosition> = None;

    for position in mouse_positions {
        let has_dims = position
            .capture_width
            .zip(position.capture_height)
            .map(|(w, h)| w.is_finite() && h.is_finite() && w > 1.0 && h > 1.0)
            .unwrap_or(false);
        if !has_dims {
            continue;
        }
        if (position.timestamp - time).abs() < 0.001 {
            return (
                position.capture_width.unwrap_or(fallback_width).max(1.0),
                position.capture_height.unwrap_or(fallback_height).max(1.0),
            );
        }
        if position.timestamp <= time {
            prev = Some(position);
            continue;
        }
        next = Some(position);
        break;
    }

    match (prev, next) {
        (Some(p), Some(n)) => {
            let dt = (n.timestamp - p.timestamp).max(0.000001);
            let t = ((time - p.timestamp) / dt).clamp(0.0, 1.0);
            let pw = p.capture_width.unwrap_or(fallback_width).max(1.0);
            let ph = p.capture_height.unwrap_or(fallback_height).max(1.0);
            let nw = n.capture_width.unwrap_or(fallback_width).max(1.0);
            let nh = n.capture_height.unwrap_or(fallback_height).max(1.0);
            (pw + (nw - pw) * t, ph + (nh - ph) * t)
        }
        (Some(p), None) => (
            p.capture_width.unwrap_or(fallback_width).max(1.0),
            p.capture_height.unwrap_or(fallback_height).max(1.0),
        ),
        (None, Some(n)) => (
            n.capture_width.unwrap_or(fallback_width).max(1.0),
            n.capture_height.unwrap_or(fallback_height).max(1.0),
        ),
        (None, None) => (fallback_width.max(1.0), fallback_height.max(1.0)),
    }
}

pub fn cancel_export() {
    println!("[Cancel] Setting EXPORT_CANCELLED flag");
    EXPORT_CANCELLED.store(true, Ordering::SeqCst);
    println!("[Cancel] Cancellation signaled");
}

pub fn warm_up_export_pipeline_when_idle() {
    let mut idle_since: Option<Instant> = None;

    loop {
        if EXPORT_GPU_WARMED.load(Ordering::SeqCst) {
            println!("[Export][Warmup] already complete, idle scheduler exiting");
            return;
        }

        let recording_active =
            IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst);
        if recording_active || EXPORT_ACTIVE.load(Ordering::SeqCst) {
            idle_since = None;
            std::thread::sleep(EXPORT_WARMUP_IDLE_POLL);
            continue;
        }

        let idle_start = idle_since.get_or_insert_with(Instant::now);
        if idle_start.elapsed() < EXPORT_WARMUP_IDLE_DELAY {
            std::thread::sleep(EXPORT_WARMUP_IDLE_POLL);
            continue;
        }

        warm_up_export_pipeline();
        return;
    }
}

pub fn warm_up_export_pipeline() {
    if EXPORT_ACTIVE.load(Ordering::SeqCst) {
        println!("[Export][Warmup] export active, skipping warm-up");
        return;
    }
    if IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst) {
        println!("[Export][Warmup] recording active, deferring warm-up");
        return;
    }
    if EXPORT_GPU_WARMED.swap(true, Ordering::SeqCst) {
        println!("[Export][Warmup] already started/skipped");
        return;
    }
    if IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst) {
        EXPORT_GPU_WARMED.store(false, Ordering::SeqCst);
        println!("[Export][Warmup] recording started during warm-up launch, deferring");
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

            let uniforms = create_uniforms(CompositorUniformParams {
                video_offset: (0.0, 0.0),
                video_scale: (1.0, 1.0),
                output_size: (warm_w as f32, warm_h as f32),
                video_size: (warm_w as f32, warm_h as f32),
                border_radius: 0.0,
                shadow_offset: 0.0,
                shadow_blur: 0.0,
                shadow_opacity: 0.0,
                gradient_color1: [0.0, 0.0, 0.0, 1.0],
                gradient_color2: [0.0, 0.0, 0.0, 1.0],
                gradient_color3: [0.0, 0.0, 0.0, 0.0],
                gradient_color4: [0.0, 0.0, 0.0, 0.0],
                gradient_color5: [0.0, 0.0, 0.0, 0.0],
                bg_params1: [0.0, 0.0, 0.0, 0.0],
                bg_params2: [0.0, 0.0, 0.0, 0.0],
                bg_params3: [0.0, 0.0, 0.0, 0.0],
                bg_params4: [0.0, 0.0, 0.0, 0.0],
                bg_params5: [0.0, 0.0, 0.0, 0.0],
                bg_params6: [0.0, 0.0, 0.0, 0.0],
                time: 0.0,
                render_mode: 0.0,
                cursor_pos: (-1.0, -1.0),
                cursor_scale: 0.0,
                cursor_opacity: 0.0,
                cursor_type_id: 0.0,
                cursor_rotation: 0.0,
                cursor_shadow: 0.0,
                use_background_texture: false,
                bg_zoom: 1.0,
                bg_anchor: (0.5, 0.5),
                background_style: 0.0,
                bg_tex_w: 0.0,
                bg_tex_h: 0.0,
            });

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
    let dx12_ok = probe_dx12();
    let mf_hw = probe_mf_h264_hardware();
    serde_json::json!({
        // pipeline degrades to cpu_fallback when DX12/wgpu cannot initialise;
        // the wgpu compositor has no CPU fallback so the export would fail anyway.
        "pipeline": if dx12_ok { "zero_copy_gpu" } else { "cpu_fallback" },
        "mf_h264": true,       // MF H.264 software encoder ships with every Win10/11
        "mf_h264_hw": mf_hw,   // hardware-accelerated path (Intel QSV / AMD VCE / NVENC via MF)
    })
}

/// Probe whether a D3D12 device can be created (minimum feature level 11.0).
/// Creating and immediately dropping the device is the only reliable check.
fn probe_dx12() -> bool {
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
    use windows::Win32::Graphics::Direct3D12::{D3D12CreateDevice, ID3D12Device};
    let mut device: Option<ID3D12Device> = None;
    unsafe { D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok()
}

/// Probe whether a hardware H.264 MFT encoder is registered on this machine.
/// Uses MFTEnumEx (enumerate-only, no instantiation) so it is cheap (<1 ms).
fn probe_mf_h264_hardware() -> bool {
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, MFMediaType_Video, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
        MFT_ENUM_FLAG_SORTANDFILTER, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
        MFVideoFormat_NV12,
    };
    use windows::Win32::System::Com::CoTaskMemFree;

    let input_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;
    let ok = unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input_info),
            Some(&output_info),
            &mut activates,
            &mut count,
        )
        .is_ok()
    };
    // Release the returned IMFActivate array (CoTaskMemAlloc'd by MF).
    if !activates.is_null() {
        for i in 0..count as usize {
            unsafe { (*activates.add(i)).take() };
        }
        unsafe { CoTaskMemFree(Some(activates as *const _)) };
    }
    ok && count > 0
}

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = ExportActiveGuard::activate();
    EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    progress::persist_replay_args(&args);

    let parse_start = Instant::now();
    let config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let parse_secs = parse_start.elapsed().as_secs_f64();
    let staged = staging::take_staged();
    let progress_cb: gpu_pipeline::ProgressCallback = Box::new(|pct, eta| {
        push_export_progress(pct, eta);
    });

    run_native_export_with_staged(config, staged, parse_secs, Some(progress_cb))
}

pub(crate) fn run_native_export_with_staged(
    mut config: ExportConfig,
    staged: staging::StagedExportData,
    parse_secs: f64,
    progress_cb: Option<gpu_pipeline::ProgressCallback>,
) -> Result<serde_json::Value, String> {
    let export_total_start = Instant::now();
    println!("[Export] Starting zero-copy GPU export...");

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
    let cam_unsorted = baked_path
        .windows(2)
        .filter(|w| w[1].time < w[0].time)
        .count();
    if cam_unsorted > 0 {
        println!(
            "[Export][WARN] Baked camera path has {} non-monotonic entries — sorting",
            cam_unsorted
        );
        baked_path.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let cur_unsorted = baked_cursor
        .windows(2)
        .filter(|w| w[1].time < w[0].time)
        .count();
    if cur_unsorted > 0 {
        println!(
            "[Export][WARN] Baked cursor path has {} non-monotonic entries — sorting",
            cur_unsorted
        );
        baked_cursor.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    println!(
        "[Export] Baked paths: camera={} frames, cursor={} frames",
        baked_path.len(),
        baked_cursor.len()
    );
    let overlay_frames = staged.overlay_frames;
    let mut webcam_frames = staged.webcam_frames;
    let webcam_unsorted = webcam_frames
        .windows(2)
        .filter(|w| w[1].time < w[0].time)
        .count();
    if webcam_unsorted > 0 {
        println!(
            "[Export][WARN] Baked webcam path has {} non-monotonic entries — sorting",
            webcam_unsorted
        );
        webcam_frames.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let cursor_slot_overrides = staged.cursor_slot_overrides;
    println!(
        "[Export] Browser cursor slot overrides: {}",
        cursor_slot_overrides.len()
    );
    // Animated cursor slots live in a persistent store (pre-computed in background,
    // survives clear_staged). Zero export-time cost.
    let animated_cursor_slots = staging::get_animated_cursor_slots();
    let atlas_rgba = staged.atlas_rgba;
    let atlas_w = staged.atlas_w;
    let atlas_h = staged.atlas_h;

    // 0. Handle Source Video/Audio — always by file path, never inline data.
    let explicit_source_video_path = config.source_video_path.trim().to_string();

    let source_video_path = if !explicit_source_video_path.is_empty()
        && Path::new(&explicit_source_video_path).exists()
    {
        explicit_source_video_path
    } else {
        VIDEO_PATH
            .lock()
            .unwrap()
            .clone()
            .ok_or("No source video found")?
    };

    let legacy_audio_volume = config.background_config.volume.clamp(0.0, 1.0);
    let mut device_audio_points = config.segment.device_audio_points.clone();
    device_audio_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if device_audio_points.is_empty() {
        device_audio_points = vec![
            config::DeviceAudioPoint {
                time: 0.0,
                volume: legacy_audio_volume,
            },
            config::DeviceAudioPoint {
                time: config.duration.max(0.0),
                volume: legacy_audio_volume,
            },
        ];
    }
    let mut mic_audio_points = config.segment.mic_audio_points.clone();
    mic_audio_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if mic_audio_points.is_empty() {
        mic_audio_points = vec![
            config::DeviceAudioPoint {
                time: 0.0,
                volume: 0.0,
            },
            config::DeviceAudioPoint {
                time: config.duration.max(0.0),
                volume: 0.0,
            },
        ];
    }
    let has_audible_device_audio = device_audio_points.iter().any(|point| point.volume > 0.0001);
    let has_audible_mic_audio = mic_audio_points.iter().any(|point| point.volume > 0.0001);
    let mut speed_points = config.segment.speed_points.clone();
    speed_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let output_base_dir = if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    };

    fs::create_dir_all(&output_base_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    let is_gif = config.format == "gif";
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    // Final output path: always uses the user-selected extension.
    let final_output_path =
        output_base_dir.join(format!("SGT_Export_{}.{}", timestamp_ms, config.format));
    // For GIF: GPU encodes a temp silent MP4, then FFmpeg converts it to a real
    // animated GIF (two-pass palettegen+paletteuse). For MP4: encode directly.
    let encode_output_path = if is_gif {
        output_base_dir.join(format!("SGT_Export_{}_tmp.mp4", timestamp_ms))
    } else {
        final_output_path.clone()
    };
    let use_preprocessed_audio =
        config.format != "gif" && !config.mic_audio_path.trim().is_empty() && has_audible_mic_audio;
    let mixed_audio_path = if use_preprocessed_audio {
        build_preprocessed_audio_mix(
            &[
                ExportAudioSource {
                    path: config.device_audio_path.clone(),
                    volume_points: device_audio_points.clone(),
                    start_offset_sec: 0.0,
                },
                ExportAudioSource {
                    path: config.mic_audio_path.clone(),
                    volume_points: mic_audio_points.clone(),
                    start_offset_sec: config.segment.mic_audio_offset_sec,
                },
            ],
            &speed_points,
            config.trim_start,
            config.duration,
            &config.segment.trim_segments,
            &output_base_dir,
            &format!("SGT_Export_{}", timestamp_ms),
        )?
    } else {
        None
    };
    let mixed_audio_cleanup_path = mixed_audio_path.clone();
    let source_audio_path = if let Some(path) = &mixed_audio_path {
        Some(path.to_string_lossy().to_string())
    } else if !config.device_audio_path.is_empty()
        && has_audible_device_audio
        && config.format != "gif"
    {
        Some(config.device_audio_path.clone())
    } else {
        None
    };
    let audio_is_preprocessed = mixed_audio_path.is_some();

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
            src_w as f64,
            src_h as f64,
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
    let out_aspect = out_w as f64 / out_h as f64;

    // Initialize GPU compositor — background uploaded at native image size later;
    // object-fit: cover is handled in the shader (no CPU pre-scaling needed).
    let gpu_init_start = Instant::now();
    let mut compositor = GpuCompositor::new(out_w, out_h, crop_w, crop_h, out_w, out_h)
        .map_err(|e| format!("GPU init failed: {}", e))?;
    let gpu_device_secs = gpu_init_start.elapsed().as_secs_f64();

    let cursor_init_start = Instant::now();
    compositor.init_cursor_texture_fast(&used_cursor_slots);
    // Frontend can stage browser-rasterized cursor tiles so export matches preview
    // exactly (avoids SVG renderer parity differences in hotspot placement).
    for override_tile in &cursor_slot_overrides {
        compositor.upload_cursor_slot_rgba(override_tile.slot_id, override_tile.rgba.as_slice());
    }
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
    let background_preset = background_presets::get_builtin_background(bg)
        .copied()
        .unwrap_or_default();
    let bg_style = background_preset.family_code;

    let shadow_opacity = if config.background_config.shadow > 0.0 {
        0.5_f32
    } else {
        0.0
    };
    let shadow_blur = config.background_config.shadow as f32;
    let border_radius = config.background_config.border_radius as f32;
    let shadow_offset = config.background_config.shadow as f32 * 0.5;
    let cursor_scale_cfg = config.background_config.cursor_scale;
    let cursor_shadow = (config.background_config.cursor_shadow as f32 / 100.0).clamp(0.0, 2.0);
    println!(
        "[Export] Cursor shadow setting: {}% (normalized {:.3})",
        config.background_config.cursor_shadow, cursor_shadow
    );
    let ow = out_w as f64;
    let oh = out_h as f64;
    let cw = crop_w as f64;
    let ch = crop_h as f64;
    let ow32 = out_w as f32;
    let oh32 = out_h as f32;

    let build_uniforms = |base_time: f64,
                          cam_pan_time: f64,
                          cam_zoom_time: f64,
                          cursor_time: f64|
     -> CompositorUniforms {
        let (cam_x_raw, cam_y_raw, _) = sample_baked_path(cam_pan_time, &baked_path);
        let (_, _, zoom) = sample_baked_path(cam_zoom_time, &baked_path);
        let cursor_sample = sample_parsed_baked_cursor(cursor_time, &parsed_baked_cursor);
        let (capture_w, capture_h) = sample_capture_dimensions_at_time(
            base_time,
            &config.mouse_positions,
            src_w as f64,
            src_h as f64,
        );
        let logical_crop_w = if let Some(c) = crop {
            (capture_w * c.width).max(1.0)
        } else {
            capture_w.max(1.0)
        };
        let logical_crop_h = if let Some(c) = crop {
            (capture_h * c.height).max(1.0)
        } else {
            capture_h.max(1.0)
        };
        let dynamic_crop_aspect = logical_crop_w / logical_crop_h.max(1.0);
        let (video_w, video_h) = if dynamic_crop_aspect > out_aspect {
            let w = ow * scale_factor;
            let h = w / dynamic_crop_aspect;
            (w.max(1.0), h.max(1.0))
        } else {
            let h = oh * scale_factor;
            let w = h * dynamic_crop_aspect;
            (w.max(1.0), h.max(1.0))
        };
        let size_ratio = (ow / logical_crop_w.max(1.0)).min(oh / logical_crop_h.max(1.0));

        let cam_x = cam_x_raw - crop_x_offset;
        let cam_y = cam_y_raw - crop_y_offset;
        let zvw = video_w * zoom;
        let zvh = video_h * zoom;
        let rx = (cam_x / cw).clamp(0.0, 1.0);
        let ry = (cam_y / ch).clamp(0.0, 1.0);
        let zsx = (1.0 - zoom) * rx;
        let zsy = (1.0 - zoom) * ry;
        let bcx = (1.0 - video_w / ow) / 2.0 * zoom;
        let bcy = (1.0 - video_h / oh) / 2.0 * zoom;
        let ox = zsx + bcx;
        let oy = zsy + bcy;

        let (cp_x, cp_y, cs, co, ct, cr) = if let Some((cx, cy, c_s, c_t, c_o, c_r)) = cursor_sample
        {
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

        create_uniforms(CompositorUniformParams {
            video_offset: (ox as f32, oy as f32),
            video_scale: (zvw as f32 / ow32, zvh as f32 / oh32),
            output_size: (ow32, oh32),
            video_size: (zvw as f32, zvh as f32),
            border_radius,
            shadow_offset,
            shadow_blur,
            shadow_opacity,
            gradient_color1: background_preset.gradient_color1,
            gradient_color2: background_preset.gradient_color2,
            gradient_color3: background_preset.gradient_color3,
            gradient_color4: background_preset.gradient_color4,
            gradient_color5: background_preset.gradient_color5,
            bg_params1: background_preset.bg_params1,
            bg_params2: background_preset.bg_params2,
            bg_params3: background_preset.bg_params3,
            bg_params4: background_preset.bg_params4,
            bg_params5: background_preset.bg_params5,
            bg_params6: background_preset.bg_params6,
            time: base_time as f32,
            render_mode: 0.0,
            cursor_pos: (cp_x, cp_y),
            cursor_scale: cs,
            cursor_opacity: co,
            cursor_type_id: ct,
            cursor_rotation: cr,
            cursor_shadow,
            use_background_texture: use_custom_background,
            bg_zoom: zoom as f32,
            bg_anchor: (rx as f32, ry as f32),
            background_style: bg_style,
            bg_tex_w: actual_bg_w,
            bg_tex_h: actual_bg_h,
        })
    };

    let bitrate = if config.target_video_bitrate_kbps > 0 {
        config.target_video_bitrate_kbps
    } else {
        config::compute_default_video_bitrate_kbps(out_w, out_h, config.framerate)
    };

    // Motion blur: derive samples and per-channel shutters from config
    let mb_zoom = config.background_config.motion_blur_zoom / 100.0;
    let mb_pan = config.background_config.motion_blur_pan / 100.0;
    let mb_cursor = config.background_config.motion_blur_cursor / 100.0;
    let mb_max = mb_zoom.max(mb_pan).max(mb_cursor);
    let mb_samples = if mb_max > 0.0001 {
        (mb_max * 8.0).ceil().clamp(2.0, 8.0) as u32
    } else {
        1
    };

    let pipeline_config = gpu_pipeline::PipelineConfig {
        source_video_path: source_video_path.clone(),
        output_path: encode_output_path.to_str().unwrap().to_string(),
        audio_path: source_audio_path.clone(),
        audio_is_preprocessed,
        audio_volume_points: device_audio_points,
        webcam_video_path: if config.webcam_video_path.trim().is_empty() {
            None
        } else {
            Some(config.webcam_video_path.clone())
        },
        webcam_offset_sec: config.segment.webcam_offset_sec,
        webcam_frames,
        output_width: out_w,
        output_height: out_h,
        framerate: config.framerate,
        bitrate_kbps: bitrate,
        speed_points,
        trim_start: config.trim_start,
        duration: config.duration,
        codec: mf_encode::VideoCodec::H264,
        trim_segments: config.segment.trim_segments.clone(),
        motion_blur_samples: mb_samples,
        blur_zoom_shutter: mb_zoom.clamp(0.0, 1.0),
        blur_pan_shutter: mb_pan.clamp(0.0, 1.0),
        blur_cursor_shutter: mb_cursor.clamp(0.0, 1.0),
        video_width: crop_w,
        video_height: crop_h,
        crop_x: crop_x_offset as u32,
        crop_y: crop_y_offset as u32,
        overlay_frames,
        animated_cursor_slots,
    };

    let source_times = gpu_pipeline::build_frame_times(&pipeline_config);
    let total_frames = source_times.len() as u32;
    let planned_output_duration_sec = total_frames as f64 / config.framerate as f64;

    println!(
        "[Export] Pipeline config: {}x{} @ {} fps, bitrate={}k, trim_start={:.3}, dur={:.3}, output_dur={:.3}",
        out_w,
        out_h,
        config.framerate,
        bitrate,
        config.trim_start,
        config.duration,
        planned_output_duration_sec
    );

    let result = gpu_pipeline::run_zero_copy_export(
        &pipeline_config,
        &mut compositor,
        &build_uniforms,
        progress_cb,
        &EXPORT_CANCELLED,
        &source_times,
    );

    let _ = mf_decode::mf_shutdown();

    match result {
        Ok(r) => {
            // If cancelled, the encode thread may have still finalized a partial file.
            // Detect this case and report cancellation instead of false success.
            if EXPORT_CANCELLED.load(Ordering::SeqCst) {
                let _ = fs::remove_file(&encode_output_path);
                if let Some(path) = &mixed_audio_cleanup_path {
                    let _ = fs::remove_file(path);
                }
                println!("[Export][Summary] status=cancelled (partial encode finalized)");
                return Ok(serde_json::json!({ "status": "cancelled" }));
            }

            // GIF: convert temp MP4 → animated GIF via FFmpeg, then remove the temp file.
            if is_gif {
                println!("[Export] Converting to GIF via FFmpeg...");
                let gif_max_w = out_w.min(960);
                match convert_mp4_to_gif(&encode_output_path, &final_output_path, gif_max_w) {
                    Ok(()) => {
                        let _ = fs::remove_file(&encode_output_path);
                        println!("[Export] GIF conversion complete");
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&encode_output_path);
                        let _ = fs::remove_file(&final_output_path);
                        if let Some(path) = &mixed_audio_cleanup_path {
                            let _ = fs::remove_file(path);
                        }
                        println!("[Export][Summary] status=error (gif convert) error={}", e);
                        return Err(e);
                    }
                }
            }

            let total_secs = export_total_start.elapsed().as_secs_f64();

            let output_bytes = fs::metadata(&final_output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let output_duration_sec =
                (r.frames_encoded as f64 / config.framerate as f64).max(0.001);
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
                "[Export][Summary] status=success out={}x{} fps={} dur={:.3}s frames={} parse={:.3}s gpu={:.3}s cursor={:.3}s total={:.3}s pipeline=zero_copy actual_kbps={:.1}",
                out_w,
                out_h,
                config.framerate,
                config.duration,
                r.frames_encoded,
                parse_secs,
                gpu_device_secs,
                cursor_init_secs,
                total_secs,
                actual_total_bitrate_kbps
            );
            if let Some(path) = &mixed_audio_cleanup_path {
                let _ = fs::remove_file(path);
            }

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
            let _ = fs::remove_file(&encode_output_path);
            let _ = fs::remove_file(&final_output_path);
            if let Some(path) = &mixed_audio_cleanup_path {
                let _ = fs::remove_file(path);
            }

            if EXPORT_CANCELLED.load(Ordering::SeqCst) {
                println!("[Export][Summary] status=cancelled");
                return Ok(serde_json::json!({ "status": "cancelled" }));
            }

            println!("[Export][Summary] status=error error={}", e);
            Err(e)
        }
    }
}

/// Path to the FFmpeg binary installed by the app setup.
fn ffmpeg_exe() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

/// Convert a silent MP4 to an animated GIF using FFmpeg's two-pass palettegen+paletteuse.
///
/// Uses a single filtergraph with `split` so FFmpeg only decodes the input once.
/// The palette is built from the entire video (`stats_mode=full`) for best global quality.
/// Bayer dithering (scale 3) gives a good quality/size balance without Floyd-Steinberg noise.
fn convert_mp4_to_gif(mp4_path: &Path, gif_path: &Path, max_width: u32) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }

    // scale=W:-1 keeps the original if the video is already narrower than max_width.
    let filter = format!(
        "scale='min({max_width},iw)':-1:flags=lanczos,\
         split[s0][s1];\
         [s0]palettegen=stats_mode=full[p];\
         [s1][p]paletteuse=dither=bayer:bayer_scale=3"
    );

    let out = std::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-i",
            mp4_path.to_str().unwrap_or(""),
            "-vf",
            &filter,
            "-loop",
            "0",
            gif_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| format!("Failed to launch FFmpeg: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("FFmpeg GIF conversion failed:\n{stderr}"));
    }

    Ok(())
}
