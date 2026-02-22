// Threaded GPU export pipeline with CPU bridge.
//
// Two threads running in parallel:
//   Decode thread:  MF decode → D3D11 VP (NV12→BGRA) → CPU readback → channel
//   Main thread:    channel → wgpu upload → compositor render → wgpu readback → MF encode
//
// Frame selection: sample-and-hold using source PTS to handle VFR sources.
// wgpu and D3D11 use completely independent devices — no D3D11On12.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

use super::d3d_interop::{create_d3d11_device, D3D11Readback, VideoProcessor};
use super::gpu_export::{CompositorUniforms, GpuCompositor};
use super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::mf_encode::{EncoderConfig, MfEncoder, VideoCodec};
use super::native_export::config::TrimSegment;
use super::native_export::sampling::output_to_source_time;

/// Result of a GPU export run.
pub struct ZeroCopyExportResult {
    pub frames_encoded: u32,
    pub elapsed_secs: f64,
    pub fps: f64,
}

/// Progress callback: (percent, eta_seconds).
pub type ProgressCallback = Box<dyn Fn(f64, f64) + Send>;

/// Full pipeline configuration.
pub struct PipelineConfig {
    pub source_video_path: String,
    pub output_path: String,
    pub output_width: u32,
    pub output_height: u32,
    pub framerate: u32,
    pub bitrate_kbps: u32,
    pub speed: f64,
    pub trim_start: f64,
    pub duration: f64,
    pub codec: VideoCodec,
    pub trim_segments: Vec<TrimSegment>,
    pub motion_blur_samples: u32,
    pub motion_blur_shutter: f64,
    /// Video texture dimensions (crop_w × crop_h from compositor).
    pub video_width: u32,
    pub video_height: u32,
    /// Crop offset in source pixels (0 if no crop).
    pub crop_x: u32,
    pub crop_y: u32,
}

/// Message sent from decode thread to render/encode thread.
struct DecodeOutput {
    bgra_video: Vec<u8>,
    source_time: f64,
    frame_idx: u32,
}

/// Run the threaded GPU export pipeline.
///
/// Spawns a decode thread that produces BGRA frames via a bounded channel.
/// The main thread consumes frames: compositor render → wgpu readback → MF encode.
/// Frame selection uses sample-and-hold with source PTS to handle VFR sources.
pub fn run_zero_copy_export(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &dyn Fn(f64, u32) -> CompositorUniforms,
    compose_overlay: &dyn Fn(f64, &mut Vec<u8>),
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> Result<ZeroCopyExportResult, String> {
    let start = Instant::now();
    let total_frames = (config.duration * config.framerate as f64 / config.speed).ceil() as u32;
    let step = config.speed / config.framerate as f64;
    let mb_samples = config.motion_blur_samples.max(1);
    let mb_enabled = mb_samples > 1 && config.motion_blur_shutter > 0.0;

    // Bounded channel: decode can run 2 frames ahead of render/encode.
    let (tx, rx) = mpsc::sync_channel::<DecodeOutput>(2);
    let decode_error: std::sync::Arc<Mutex<Option<String>>> =
        std::sync::Arc::new(Mutex::new(None));

    println!(
        "[Pipeline] {} frames, {}x{} → {}x{} @ {}fps, blur={}(shutter={:.2}, mb={}), segs={}",
        total_frames,
        config.video_width,
        config.video_height,
        config.output_width,
        config.output_height,
        config.framerate,
        config.motion_blur_samples,
        config.motion_blur_shutter,
        mb_enabled,
        config.trim_segments.len()
    );

    let mut result: Result<ZeroCopyExportResult, String> = Err("pipeline did not run".into());
    let decode_err_clone = decode_error.clone();

    std::thread::scope(|s| {
        s.spawn(move || {
            if let Err(e) = run_decode_thread(config, cancel_flag, total_frames, step, tx) {
                cancel_flag.store(true, Ordering::Relaxed);
                *decode_err_clone.lock().unwrap() = Some(e);
            }
        });

        result = run_render_encode(
            config,
            compositor,
            build_uniforms,
            compose_overlay,
            progress,
            cancel_flag,
            &rx,
            total_frames,
            step,
            mb_samples,
            mb_enabled,
            &start,
        );

        if result.is_err() {
            cancel_flag.store(true, Ordering::Relaxed);
        }
    });

    // Check for decode thread error (may have caused early exit)
    if let Some(decode_err) = decode_error.lock().unwrap().take() {
        if result.is_ok() {
            return Err(format!("Decode thread: {decode_err}"));
        }
    }

    result
}

/// Decode thread: creates its own D3D11 device, decodes with sample-and-hold frame selection.
///
/// Uses source PTS to handle VFR: holds each decoded frame until the next one's
/// timestamp is needed, duplicating frames when output fps > source fps.
fn run_decode_thread(
    config: &PipelineConfig,
    cancel_flag: &std::sync::atomic::AtomicBool,
    total_frames: u32,
    step: f64,
    tx: mpsc::SyncSender<DecodeOutput>,
) -> Result<(), String> {
    let t_thread = Instant::now();

    // --- D3D11 device #1 (decode thread owns this) ---
    let (d3d11_device, d3d11_context) = create_d3d11_device()?;

    // Enable multithread protection (MF decoder may use internal threads)
    {
        let mt: ID3D11Multithread = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread: {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }

    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;
    let decoder = MfDecoder::new(&config.source_video_path, &device_manager, true)?;
    let source_w = decoder.width();
    let source_h = decoder.height();

    // Seek to start
    let initial_seek = if !config.trim_segments.is_empty() {
        config.trim_segments[0].start_time
    } else {
        config.trim_start
    };
    if initial_seek > 0.0 {
        decoder.seek_seconds(initial_seek)?;
    }

    // VideoProcessor (NV12→BGRA, with optional crop)
    let vp_out_w = config.video_width;
    let vp_out_h = config.video_height;
    let decode_vp = VideoProcessor::new(
        &d3d11_device,
        &d3d11_context,
        source_w,
        source_h,
        vp_out_w,
        vp_out_h,
    )?;

    // Always set source rect (NV12 textures are 16-pixel height-aligned)
    decode_vp.set_source_rect(config.crop_x, config.crop_y, vp_out_w, vp_out_h);

    let vp_output = VideoProcessor::create_texture(
        &d3d11_device,
        vp_out_w,
        vp_out_h,
        DXGI_FORMAT_B8G8R8A8_UNORM,
        D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
    )?;

    let readback = D3D11Readback::new(
        &d3d11_device,
        &d3d11_context,
        vp_out_w,
        vp_out_h,
        DXGI_FORMAT_B8G8R8A8_UNORM,
    )?;

    let has_trim = !config.trim_segments.is_empty();
    let mut current_segment_idx: usize = 0;

    // Decode one source frame: MF decode → VP convert → CPU readback → return PTS
    let decode_one = |buf: &mut Vec<u8>| -> Result<Option<f64>, String> {
        let decoded = match decoder.read_frame()? {
            Some(f) => f,
            None => return Ok(None),
        };
        decode_vp.convert(&decoded.texture, decoded.subresource_index, &vp_output)?;
        readback.readback(&vp_output, buf)?;
        Ok(Some(decoded.pts_100ns as f64 / 10_000_000.0))
    };

    // ─── Sample-and-hold frame selection ───
    // Decode source frames and hold each until the next one's PTS is needed.
    // This handles VFR sources by duplicating frames when output fps > source fps.

    let mut cur_bgra: Vec<u8> = Vec::new();
    let mut cur_pts: f64 = match decode_one(&mut cur_bgra)? {
        Some(pts) => pts,
        None => return Ok(()),
    };

    let mut next_bgra: Vec<u8> = Vec::new();
    let mut next_pts: f64 = f64::MAX;
    let mut have_next = false;
    let mut src_decoded: u32 = 1;

    if let Some(pts) = decode_one(&mut next_bgra)? {
        next_pts = pts;
        have_next = true;
        src_decoded += 1;
    }

    let mut frames_held: u32 = 0;

    // PTS diagnostics
    let mut last_decoded_pts: f64 = cur_pts;
    let mut non_mono: u32 = 0; // PTS went backwards
    let mut max_gap_ms: f64 = 0.0;
    let mut max_hold_streak: u32 = 0;
    let mut hold_streak: u32 = 0;
    let mut max_skip: u32 = 0; // max source frames skipped in one advance loop
    let mut max_drift_ms: f64 = 0.0; // max |source_time - cur_pts|

    // Track PTS after each decode
    let track_pts = |pts: f64, last: &mut f64, non_mono: &mut u32, max_gap: &mut f64| {
        if pts < *last - 0.001 {
            *non_mono += 1;
        }
        let gap = (pts - *last).abs() * 1000.0;
        if gap > *max_gap {
            *max_gap = gap;
        }
        *last = pts;
    };

    for frame_idx in 0..total_frames {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        // Map output time → source time
        let output_time = frame_idx as f64 * step;
        let source_time = if has_trim {
            output_to_source_time(output_time, &config.trim_segments, config.trim_start)
        } else {
            config.trim_start + output_time
        };

        // Seek on trim segment boundary change
        if has_trim {
            let target_seg = config
                .trim_segments
                .iter()
                .position(|s| {
                    source_time >= s.start_time - 0.001 && source_time <= s.end_time + 0.001
                })
                .unwrap_or(current_segment_idx);

            if target_seg != current_segment_idx {
                decoder.seek_seconds(config.trim_segments[target_seg].start_time)?;
                current_segment_idx = target_seg;
                // Re-decode current + next after seek
                cur_pts = match decode_one(&mut cur_bgra)? {
                    Some(pts) => pts,
                    None => break,
                };
                src_decoded += 1;
                track_pts(cur_pts, &mut last_decoded_pts, &mut non_mono, &mut max_gap_ms);
                if let Some(pts) = decode_one(&mut next_bgra)? {
                    next_pts = pts;
                    have_next = true;
                    src_decoded += 1;
                    track_pts(pts, &mut last_decoded_pts, &mut non_mono, &mut max_gap_ms);
                } else {
                    have_next = false;
                    next_pts = f64::MAX;
                }
            }
        }

        // Advance decoder until we find the best frame for source_time.
        // "Best frame" = latest decoded frame whose PTS ≤ source_time.
        // We advance as long as next_pts ≤ source_time (the next frame is
        // already valid or earlier), then cur becomes the best match.
        let mut skip_count: u32 = 0;
        while have_next && next_pts <= source_time {
            std::mem::swap(&mut cur_bgra, &mut next_bgra);
            cur_pts = next_pts;
            skip_count += 1;
            match decode_one(&mut next_bgra)? {
                Some(pts) => {
                    next_pts = pts;
                    src_decoded += 1;
                    track_pts(pts, &mut last_decoded_pts, &mut non_mono, &mut max_gap_ms);
                }
                None => {
                    have_next = false;
                    next_pts = f64::MAX;
                }
            }
        }

        if skip_count > max_skip {
            max_skip = skip_count;
        }

        if skip_count == 0 && frame_idx > 0 {
            frames_held += 1;
            hold_streak += 1;
            if hold_streak > max_hold_streak {
                max_hold_streak = hold_streak;
            }
        } else {
            hold_streak = 0;
        }

        // Track drift between desired source_time and actual frame PTS
        let drift = (source_time - cur_pts).abs() * 1000.0;
        if drift > max_drift_ms {
            max_drift_ms = drift;
        }

        let msg = DecodeOutput {
            bgra_video: cur_bgra.clone(),
            source_time,
            frame_idx,
        };
        if tx.send(msg).is_err() {
            break;
        }
    }

    let elapsed = t_thread.elapsed().as_secs_f64();
    println!(
        "[Decode] {} src → {} out ({} held, max_streak {}) in {:.1}s | max_gap {:.0}ms, non_mono {}, max_skip {}, max_drift {:.0}ms",
        src_decoded,
        total_frames,
        frames_held,
        max_hold_streak,
        elapsed,
        max_gap_ms,
        non_mono,
        max_skip,
        max_drift_ms,
    );

    Ok(())
}

/// Main thread: receives decoded BGRA frames, renders via compositor, encodes.
#[allow(clippy::too_many_arguments)]
fn run_render_encode(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &dyn Fn(f64, u32) -> CompositorUniforms,
    compose_overlay: &dyn Fn(f64, &mut Vec<u8>),
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    rx: &mpsc::Receiver<DecodeOutput>,
    total_frames: u32,
    step: f64,
    mb_samples: u32,
    mb_enabled: bool,
    start: &Instant,
) -> Result<ZeroCopyExportResult, String> {
    // --- D3D11 device #2 (main thread, for encoder HW acceleration) ---
    let (enc_device, _enc_context) = create_d3d11_device()?;
    let enc_device_manager = DxgiDeviceManager::new(&enc_device)?;

    let encoder_config = EncoderConfig {
        codec: config.codec,
        width: config.output_width,
        height: config.output_height,
        fps_num: config.framerate,
        fps_den: 1,
        bitrate_kbps: config.bitrate_kbps,
    };
    let encoder = MfEncoder::new(&config.output_path, encoder_config, &enc_device_manager)?;
    let frame_duration_100ns = encoder.frame_duration_100ns();

    let mut overlay_buf = vec![0u8; (config.output_width * config.output_height * 4) as usize];
    let mut output_buf: Vec<u8> = Vec::new();
    let mut frames_encoded: u32 = 0;

    // Bottleneck timing accumulators
    let mut t_compose = 0.0_f64;
    let mut t_render = 0.0_f64;
    let mut t_encode = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // Diagnostic: bypass wgpu compositor — feed decoded BGRA directly to encoder.
    // Set SGT_BYPASS_COMPOSITOR=1 env var to test if blinking comes from wgpu or D3D11/MF.
    let bypass_compositor = std::env::var("SGT_BYPASS_COMPOSITOR").is_ok();
    if bypass_compositor {
        println!("[Pipeline] BYPASS_COMPOSITOR mode — skipping wgpu, raw decode→encode");
    }

    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        t_wait += tw0.elapsed().as_secs_f64();

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        if bypass_compositor {
            let te0 = Instant::now();
            let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;
            encoder.write_frame_cpu(&msg.bgra_video, timestamp_100ns, frame_duration_100ns)?;
            t_encode += te0.elapsed().as_secs_f64();
            frames_encoded += 1;
        } else {
            // Upload video + compose overlay
            let tc0 = Instant::now();
            compositor.upload_frame(&msg.bgra_video);
            overlay_buf.fill(0);
            compose_overlay(msg.source_time, &mut overlay_buf);
            compositor.upload_overlay(&overlay_buf);
            t_compose += tc0.elapsed().as_secs_f64();

            // Render (with optional motion blur) + readback from wgpu
            let tr0 = Instant::now();
            if mb_enabled {
                let half_shutter = step * config.motion_blur_shutter * 0.5;
                for i in 0..mb_samples {
                    let t = if mb_samples > 1 {
                        i as f64 / (mb_samples - 1) as f64
                    } else {
                        0.5
                    };
                    let sub_time = msg.source_time - half_shutter + t * 2.0 * half_shutter;
                    let uniforms = build_uniforms(sub_time, msg.frame_idx);
                    let weight = 1.0 / (i as f64 + 1.0);
                    compositor.render_accumulate(&uniforms, i == 0, weight);
                }
                compositor.enqueue_output_readback()?;
                compositor.readback_output(&mut output_buf)?;
            } else {
                let uniforms = build_uniforms(msg.source_time, msg.frame_idx);
                compositor.render_frame_into(&uniforms, &mut output_buf)?;
            }
            t_render += tr0.elapsed().as_secs_f64();

            // Encode BGRA from CPU buffer
            let te0 = Instant::now();
            let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;
            encoder.write_frame_cpu(&output_buf, timestamp_100ns, frame_duration_100ns)?;
            t_encode += te0.elapsed().as_secs_f64();
            frames_encoded += 1;
        }

        // Frontend progress callback
        if let Some(ref cb) = progress {
            if frames_encoded.is_multiple_of(15) || frames_encoded == total_frames {
                let elapsed = start.elapsed().as_secs_f64();
                let pct = (frames_encoded as f64 / total_frames as f64 * 100.0).min(100.0);
                let eta = if frames_encoded > 0 {
                    (elapsed / frames_encoded as f64) * (total_frames - frames_encoded) as f64
                } else {
                    0.0
                };
                cb(pct, eta);
            }
        }
    }

    // Finalize MP4
    encoder.finalize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frames_encoded as f64 / elapsed;
    let n = frames_encoded.max(1) as f64;

    println!(
        "[Render] avg {:.1}ms/frame (compose {:.1} + render {:.1} + encode {:.1}) wait {:.1}ms",
        (t_compose + t_render + t_encode) / n * 1000.0,
        t_compose / n * 1000.0,
        t_render / n * 1000.0,
        t_encode / n * 1000.0,
        t_wait / n * 1000.0,
    );
    println!(
        "[Pipeline] Done: {} frames in {:.2}s ({:.1} fps)",
        frames_encoded, elapsed, fps
    );

    Ok(ZeroCopyExportResult {
        frames_encoded,
        elapsed_secs: elapsed,
        fps,
    })
}
