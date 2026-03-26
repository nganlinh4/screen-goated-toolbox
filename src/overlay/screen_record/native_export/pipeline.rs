use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::audio_mix::{ExportAudioSource, build_preprocessed_audio_mix};
use super::config::{self, ExportConfig};
use super::{camera_path, cursor_path, staging};

use super::gpu_pipeline;
use super::mf_decode;
use super::mf_encode;
use super::overlay_frames;
use super::pipeline_build;
use super::progress::push_export_progress;
use crate::overlay::screen_record::engine::VIDEO_PATH;

use super::{EXPORT_CANCELLED, ExportActiveGuard};

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = ExportActiveGuard::activate();
    EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    super::progress::persist_replay_args(&args);

    let parse_start = Instant::now();
    let config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let parse_secs = parse_start.elapsed().as_secs_f64();
    eprintln!("[Export][Timing] JSON parse: {:.3}s", parse_secs);
    let staged_start = Instant::now();
    let staged = staging::take_staged();
    eprintln!(
        "[Export][Timing] take_staged: {:.3}s",
        staged_start.elapsed().as_secs_f64()
    );
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

    // Destructure staged data in one step to avoid partial-move issues
    let staging::StagedExportData {
        camera_frames: staged_camera_frames,
        cursor_frames: staged_cursor_frames,
        mut webcam_frames,
        cursor_slot_overrides,
        atlas_rgba,
        atlas_w,
        atlas_h,
        overlay_frames,
        overlay_metadata,
    } = staged;

    let mut baked_path = match config.baked_path.take() {
        Some(v) if !v.is_empty() => v,
        _ => staged_camera_frames,
    };
    let mut baked_cursor = match config.baked_cursor_path.take() {
        Some(v) if !v.is_empty() => v,
        _ => staged_cursor_frames,
    };

    // Ensure baked paths are sorted by time (partition_point requires sorted input).
    sort_baked_paths(&mut baked_path, &mut baked_cursor);

    println!(
        "[Export] Baked paths: camera={} frames, cursor={} frames",
        baked_path.len(),
        baked_cursor.len()
    );

    let webcam_unsorted = webcam_frames
        .windows(2)
        .filter(|w| w[1].time < w[0].time)
        .count();
    if webcam_unsorted > 0 {
        println!(
            "[Export][WARN] Baked webcam path has {} non-monotonic entries -- sorting",
            webcam_unsorted
        );
        webcam_frames.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    println!(
        "[Export] Browser cursor slot overrides: {}",
        cursor_slot_overrides.len()
    );
    let animated_cursor_slots = staging::get_animated_cursor_slots();

    // 0. Handle Source Video/Audio
    let (source_video_path, source_audio_path, mixed_audio_cleanup_path, audio_is_preprocessed,
         device_audio_points, speed_points) =
        prepare_audio_and_video(&config)?;

    // Get source dimensions via MF SourceReader
    let t_mf_start = Instant::now();
    mf_decode::mf_startup()?;
    let (src_w, src_h) = if config.source_width > 0 && config.source_height > 0 {
        (config.source_width, config.source_height)
    } else {
        mf_decode::probe_video_dimensions(&source_video_path)?
    };
    eprintln!(
        "[Export][Timing] MF startup + probe: {:.3}s",
        t_mf_start.elapsed().as_secs_f64()
    );
    println!("[Export] Source dimensions: {}x{}", src_w, src_h);

    // Generate camera path in Rust if not provided by frontend.
    let baked_path = generate_camera_path_if_needed(baked_path, &config, src_w, src_h);

    // Generate cursor path in Rust if not provided by frontend.
    let baked_cursor = generate_cursor_path_if_needed(baked_cursor, &config, src_w, src_h);

    // Calculate dimensions
    let crop = &config.segment.crop;
    let crop_w = if let Some(c) = crop { (src_w as f64 * c.width) as u32 } else { src_w };
    let crop_h = if let Some(c) = crop { (src_h as f64 * c.height) as u32 } else { src_h };
    let crop_x_offset = if let Some(c) = crop { src_w as f64 * c.x } else { 0.0 };
    let crop_y_offset = if let Some(c) = crop { src_h as f64 * c.y } else { 0.0 };

    let out_w = if config.width == 0 { crop_w } else { config.width };
    let out_h = if config.height == 0 { crop_h } else { config.height };
    let out_w = out_w - (out_w % 2);
    let out_h = out_h - (out_h % 2);

    // Setup GPU compositor and uniform builder
    let gpu_result = pipeline_build::setup_gpu_and_uniforms(
        &config, &baked_path, &baked_cursor,
        &cursor_slot_overrides, &atlas_rgba, atlas_w, atlas_h,
        src_w, src_h, crop_w, crop_h, crop_x_offset, crop_y_offset, out_w, out_h,
    )?;
    let mut compositor = gpu_result.compositor;
    let gpu_device_secs = gpu_result.gpu_device_secs;
    let cursor_init_secs = gpu_result.cursor_init_secs;
    let uniform_params = gpu_result.uniform_params;

    let build_uniforms = |base_time: f64,
                          cam_pan_time: f64,
                          cam_zoom_time: f64,
                          cursor_time: f64|
     -> super::gpu_export::CompositorUniforms {
        uniform_params.build_uniforms(&config, base_time, cam_pan_time, cam_zoom_time, cursor_time)
    };

    let bitrate = if config.target_video_bitrate_kbps > 0 {
        config.target_video_bitrate_kbps
    } else {
        config::compute_default_video_bitrate_kbps(out_w, out_h, config.framerate)
    };

    // Motion blur
    let mb_zoom = config.background_config.motion_blur_zoom / 100.0;
    let mb_pan = config.background_config.motion_blur_pan / 100.0;
    let mb_cursor = config.background_config.motion_blur_cursor / 100.0;
    let mb_max = mb_zoom.max(mb_pan).max(mb_cursor);
    let mb_samples = if mb_max > 0.0001 {
        (mb_max * 8.0).ceil().clamp(2.0, 8.0) as u32
    } else {
        1
    };

    // Generate overlay frames in Rust if atlas metadata was sent
    let overlay_frames = if let Some(ref meta) = overlay_metadata
        && overlay_frames.is_empty()
    {
        let t0 = Instant::now();
        let generated = overlay_frames::generate_overlay_frames(
            meta,
            &config.segment.trim_segments,
            &speed_points,
            config.framerate,
            out_w as f64,
            out_h as f64,
        );
        eprintln!(
            "[Export][Timing] Rust overlay frame gen: {:.3}s ({} frames, {} non-empty)",
            t0.elapsed().as_secs_f64(),
            generated.len(),
            generated.iter().filter(|f| !f.quads.is_empty()).count()
        );
        generated
    } else {
        overlay_frames
    };

    // Build output paths
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
    let final_output_path =
        output_base_dir.join(format!("SGT_Export_{}.{}", timestamp_ms, config.format));
    let encode_output_path = if is_gif {
        output_base_dir.join(format!("SGT_Export_{}_tmp.mp4", timestamp_ms))
    } else {
        final_output_path.clone()
    };

    let mut pipeline_config = gpu_pipeline::PipelineConfig {
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

    let t_frame_times_start = Instant::now();
    let source_times = gpu_pipeline::build_frame_times(&pipeline_config);
    let total_frames = source_times.len() as u32;
    let planned_output_duration_sec = total_frames as f64 / config.framerate as f64;

    // Expand sparse overlay frames into dense array
    expand_sparse_overlay_frames(&mut pipeline_config, total_frames);

    eprintln!(
        "[Export][Timing] build_frame_times: {:.3}s ({} frames)",
        t_frame_times_start.elapsed().as_secs_f64(),
        total_frames
    );
    eprintln!(
        "[Export][Timing] TOTAL preparation (Rust-side): {:.3}s",
        export_total_start.elapsed().as_secs_f64()
    );

    println!(
        "[Export] Pipeline config: {}x{} @ {} fps, bitrate={}k, trim_start={:.3}, dur={:.3}, output_dur={:.3}",
        out_w, out_h, config.framerate, bitrate, config.trim_start, config.duration,
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

    pipeline_build::handle_export_result(
        result,
        &config,
        &encode_output_path,
        &final_output_path,
        &mixed_audio_cleanup_path,
        is_gif,
        out_w,
        out_h,
        bitrate,
        parse_secs,
        gpu_device_secs,
        cursor_init_secs,
        export_total_start,
    )
}

fn sort_baked_paths(
    baked_path: &mut [config::BakedCameraFrame],
    baked_cursor: &mut [config::BakedCursorFrame],
) {
    let cam_unsorted = baked_path
        .windows(2)
        .filter(|w| w[1].time < w[0].time)
        .count();
    if cam_unsorted > 0 {
        println!(
            "[Export][WARN] Baked camera path has {} non-monotonic entries -- sorting",
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
            "[Export][WARN] Baked cursor path has {} non-monotonic entries -- sorting",
            cur_unsorted
        );
        baked_cursor.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

type AudioVideoPrep = (
    String,
    Option<String>,
    Option<PathBuf>,
    bool,
    Vec<config::DeviceAudioPoint>,
    Vec<config::SpeedPoint>,
);

fn prepare_audio_and_video(config: &ExportConfig) -> Result<AudioVideoPrep, String> {
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
            config::DeviceAudioPoint { time: 0.0, volume: legacy_audio_volume },
            config::DeviceAudioPoint { time: config.duration.max(0.0), volume: legacy_audio_volume },
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
            config::DeviceAudioPoint { time: 0.0, volume: 0.0 },
            config::DeviceAudioPoint { time: config.duration.max(0.0), volume: 0.0 },
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

    let t_audio_start = Instant::now();
    let use_preprocessed_audio =
        config.format != "gif" && !config.mic_audio_path.trim().is_empty() && has_audible_mic_audio;
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
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
    eprintln!(
        "[Export][Timing] Audio preprocessing: {:.3}s (preprocessed={})",
        t_audio_start.elapsed().as_secs_f64(),
        audio_is_preprocessed
    );

    Ok((
        source_video_path,
        source_audio_path,
        mixed_audio_cleanup_path,
        audio_is_preprocessed,
        device_audio_points,
        speed_points,
    ))
}

fn generate_camera_path_if_needed(
    baked_path: Vec<config::BakedCameraFrame>,
    config: &ExportConfig,
    src_w: u32,
    src_h: u32,
) -> Vec<config::BakedCameraFrame> {
    if baked_path.is_empty()
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
    }
}

fn generate_cursor_path_if_needed(
    baked_cursor: Vec<config::BakedCursorFrame>,
    config: &ExportConfig,
    src_w: u32,
    src_h: u32,
) -> Vec<config::BakedCursorFrame> {
    if baked_cursor.is_empty() && !config.mouse_positions.is_empty() {
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
    }
}

fn expand_sparse_overlay_frames(
    pipeline_config: &mut gpu_pipeline::PipelineConfig,
    total_frames: u32,
) {
    let is_sparse = !pipeline_config.overlay_frames.is_empty()
        && pipeline_config
            .overlay_frames
            .iter()
            .any(|f| f.frame_index.is_some());
    if is_sparse {
        let sparse = std::mem::take(&mut pipeline_config.overlay_frames);
        let mut dense = Vec::with_capacity(total_frames as usize);
        dense.resize_with(total_frames as usize, || config::OverlayFrame {
            frame_index: None,
            quads: Vec::new(),
        });
        for frame in sparse {
            if let Some(idx) = frame.frame_index
                && (idx as usize) < dense.len()
            {
                dense[idx as usize] = frame;
            }
        }
        pipeline_config.overlay_frames = dense;
    }
}
