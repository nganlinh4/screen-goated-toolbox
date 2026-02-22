// Threaded GPU export pipeline with CPU bridge.
//
// Three threads running in parallel:
//   Decode thread:  MF decode → D3D11 VP (NV12→BGRA) → CPU readback → channel
//   Render thread:  channel → wgpu upload → compositor render → pipelined readback → channel
//   Main thread:    channel → MF encode → MP4
//
// Frame selection: sample-and-hold using source PTS to handle VFR sources.
// wgpu and D3D11 use completely independent devices — no D3D11On12.
//
// Buffer recycling:
//   send_buf  (video_w×h×4):  decode → render → (recycle) → decode
//   out_buf   (out_w×h×4):    render → encode → (recycle) → render
// Pipelined readbacks: render enqueues readback N then immediately processes
// frame N+1; readback N is drained before N+1's readback — GPU and CPU overlap.

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
use super::native_export::config::{OverlayFrame, TrimSegment};
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
    /// Per-channel blur isolation: when false, that channel samples base_time (no blur).
    pub blur_zoom: bool,
    pub blur_pan: bool,
    pub blur_cursor: bool,
    /// Video texture dimensions (crop_w × crop_h from compositor).
    pub video_width: u32,
    pub video_height: u32,
    /// Crop offset in source pixels (0 if no crop).
    pub crop_x: u32,
    pub crop_y: u32,
    /// Pre-computed overlay quads per output frame (indexed by frame_idx).
    /// Empty when there are no text/keystroke overlays.
    pub overlay_frames: Vec<OverlayFrame>,
}

/// Message sent from decode thread to render thread.
struct DecodeOutput {
    /// Recycled send buffer (video_w×h×4 BGRA). Returned to decode via recycle after GPU upload.
    bgra_video: Vec<u8>,
    source_time: f64,
    frame_idx: u32,
}

/// Message sent from render thread to encode thread.
struct RenderOutput {
    /// Recycled output buffer (out_w×h×4 BGRA). Returned to render via recycle after encode.
    rendered_bgra: Vec<u8>,
}

/// Run the threaded GPU export pipeline.
pub fn run_zero_copy_export(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &(dyn Fn(f64, f64, f64, f64) -> CompositorUniforms + Sync),
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> Result<ZeroCopyExportResult, String> {
    let start = Instant::now();
    let total_frames = (config.duration * config.framerate as f64 / config.speed).ceil() as u32;
    let step = config.speed / config.framerate as f64;
    let mb_samples = config.motion_blur_samples.max(1);
    let mb_enabled = mb_samples > 1 && config.motion_blur_shutter > 0.0;

    // Forward channels (decode → render → encode).
    let (decode_tx, render_rx) = mpsc::sync_channel::<DecodeOutput>(3);
    let (render_tx, encode_rx) = mpsc::sync_channel::<RenderOutput>(3);
    // Recycle channels (backwards): buffers return to their producer thread for reuse.
    let (render_to_decode_tx, render_to_decode_rx) = mpsc::channel::<Vec<u8>>();
    let (encode_to_render_tx, encode_to_render_rx) = mpsc::channel::<Vec<u8>>();

    let decode_error: std::sync::Arc<Mutex<Option<String>>> =
        std::sync::Arc::new(Mutex::new(None));
    let render_error: std::sync::Arc<Mutex<Option<String>>> =
        std::sync::Arc::new(Mutex::new(None));

    let mut result: Result<ZeroCopyExportResult, String> = Err("pipeline did not run".into());
    let decode_err_clone = decode_error.clone();
    let render_err_clone = render_error.clone();

    println!(
        "[Pipeline] {} frames, {}x{} → {}x{} @ {}fps, blur={} (shutter={:.2}, mb={}), segs={}",
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

    std::thread::scope(|s| {
        // Thread 1: Decode
        s.spawn(move || {
            if let Err(e) = run_decode_thread(
                config,
                cancel_flag,
                total_frames,
                step,
                decode_tx,
                render_to_decode_rx,
            ) {
                cancel_flag.store(true, Ordering::Relaxed);
                *decode_err_clone.lock().unwrap() = Some(e);
            }
        });

        // Thread 2: Render (compositor)
        s.spawn(move || {
            if let Err(e) = run_render_thread(
                config,
                compositor,
                build_uniforms,
                cancel_flag,
                render_rx,
                step,
                mb_samples,
                mb_enabled,
                render_tx,
                render_to_decode_tx,
                encode_to_render_rx,
            ) {
                cancel_flag.store(true, Ordering::Relaxed);
                *render_err_clone.lock().unwrap() = Some(e);
            }
        });

        // Main thread: Encode
        result = run_encode_thread(
            config,
            progress,
            cancel_flag,
            &encode_rx,
            total_frames,
            &start,
            encode_to_render_tx,
        );

        if result.is_err() {
            cancel_flag.store(true, Ordering::Relaxed);
        }
    });

    if let Some(e) = decode_error.lock().unwrap().take() {
        if result.is_ok() {
            return Err(format!("Decode thread: {e}"));
        }
    }
    if let Some(e) = render_error.lock().unwrap().take() {
        if result.is_ok() {
            return Err(format!("Render thread: {e}"));
        }
    }

    result
}

/// Decode thread: creates its own D3D11 device; decodes with sample-and-hold frame selection.
///
/// `cur_bgra` and `next_bgra` are PERMANENT buffers owned by this thread (never sent across
/// threads). Per output frame we copy `cur_bgra` into a recycled `send_buf` and send that.
/// This correctly handles the "hold" case (same source frame reused for multiple output frames)
/// which occurs whenever output fps > source fps.
fn run_decode_thread(
    config: &PipelineConfig,
    cancel_flag: &std::sync::atomic::AtomicBool,
    total_frames: u32,
    step: f64,
    tx: mpsc::SyncSender<DecodeOutput>,
    recycle_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), String> {
    let t_thread = Instant::now();

    // D3D11 device #1 (decode thread owns this)
    let (d3d11_device, d3d11_context) = create_d3d11_device()?;
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

    let initial_seek = if !config.trim_segments.is_empty() {
        config.trim_segments[0].start_time
    } else {
        config.trim_start
    };
    if initial_seek > 0.0 {
        decoder.seek_seconds(initial_seek)?;
    }

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

    let frame_size = (vp_out_w * vp_out_h * 4) as usize;

    // Decode one source frame into a buffer; returns PTS in seconds.
    let decode_one = |buf: &mut Vec<u8>| -> Result<Option<f64>, String> {
        let decoded = match decoder.read_frame()? {
            Some(f) => f,
            None => return Ok(None),
        };
        decode_vp.convert(&decoded.texture, decoded.subresource_index, &vp_output)?;
        readback.readback(&vp_output, buf)?;
        Ok(Some(decoded.pts_100ns as f64 / 10_000_000.0))
    };

    // Permanent hold buffers — never leave this thread.
    let mut cur_bgra: Vec<u8> = Vec::with_capacity(frame_size);
    let mut cur_pts: f64 = match decode_one(&mut cur_bgra)? {
        Some(pts) => pts,
        None => return Ok(()),
    };

    let mut next_bgra: Vec<u8> = Vec::with_capacity(frame_size);
    let mut next_pts = f64::MAX;
    let mut have_next = false;
    if let Some(pts) = decode_one(&mut next_bgra)? {
        next_pts = pts;
        have_next = true;
    }

    let has_trim = !config.trim_segments.is_empty();
    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;
    let mut src_decoded: u32 = if have_next { 2 } else { 1 };

    for frame_idx in 0..total_frames {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let output_time = frame_idx as f64 * step;
        let source_time = if has_trim {
            output_to_source_time(output_time, &config.trim_segments, config.trim_start)
        } else {
            config.trim_start + output_time
        };

        // Seek on trim segment boundary change.
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
                cur_pts = match decode_one(&mut cur_bgra)? {
                    Some(pts) => pts,
                    None => break,
                };
                src_decoded += 1;
                if let Some(pts) = decode_one(&mut next_bgra)? {
                    next_pts = pts;
                    have_next = true;
                    src_decoded += 1;
                } else {
                    have_next = false;
                    next_pts = f64::MAX;
                }
            }
        }

        // Advance: find the best source frame for this output time.
        // Fast-forward: if source_time is >1.5s ahead of the next frame (high-speed timelapse),
        // seek directly instead of decoding every intermediate frame sequentially.
        if have_next && source_time - next_pts > 1.5 {
            decoder.seek_seconds(source_time)?;
            cur_pts = match decode_one(&mut cur_bgra)? {
                Some(pts) => pts,
                None => break,
            };
            src_decoded += 1;
            if let Some(pts) = decode_one(&mut next_bgra)? {
                next_pts = pts;
                have_next = true;
                src_decoded += 1;
            } else {
                have_next = false;
                next_pts = f64::MAX;
            }
        }

        let mut advanced = false;
        while have_next && next_pts <= source_time {
            std::mem::swap(&mut cur_bgra, &mut next_bgra);
            cur_pts = next_pts;
            advanced = true;
            match decode_one(&mut next_bgra)? {
                Some(pts) => {
                    next_pts = pts;
                    src_decoded += 1;
                }
                None => {
                    have_next = false;
                    next_pts = f64::MAX;
                }
            }
        }

        if !advanced && frame_idx > 0 {
            frames_held += 1;
        }
        let _ = cur_pts; // suppress unused warning

        // Copy cur_bgra into a recycled send buffer (avoids per-frame allocation).
        // cur_bgra STAYS in this thread so holds (same frame reused for multiple outputs) work.
        let mut send_buf = recycle_rx
            .try_recv()
            .unwrap_or_else(|_| Vec::with_capacity(frame_size));
        send_buf.resize(frame_size, 0);
        send_buf.copy_from_slice(&cur_bgra);

        if tx.send(DecodeOutput { bgra_video: send_buf, source_time, frame_idx }).is_err() {
            break;
        }
    }

    let elapsed = t_thread.elapsed().as_secs_f64();
    println!(
        "[Decode] {} src → {} out ({} held) in {:.1}s",
        src_decoded, total_frames, frames_held, elapsed
    );
    Ok(())
}

/// Render thread: receives BGRA frames, runs compositor, sends rendered BGRA to encoder.
///
/// Pipelined readback depth 2: after enqueueing readback for frame N, we process frame N+1
/// before draining readback N. The GPU renders N while the CPU uploads and renders N+1.
#[allow(clippy::too_many_arguments)]
fn run_render_thread(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &(dyn Fn(f64, f64, f64, f64) -> CompositorUniforms + Sync),
    cancel_flag: &std::sync::atomic::AtomicBool,
    rx: mpsc::Receiver<DecodeOutput>,
    step: f64,
    mb_samples: u32,
    mb_enabled: bool,
    tx: mpsc::SyncSender<RenderOutput>,
    recycle_decode_tx: mpsc::Sender<Vec<u8>>,
    recycle_render_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), String> {
    let mut frames_rendered: u32 = 0;
    let mut t_upload = 0.0_f64;
    let mut t_render = 0.0_f64;
    let mut t_readback = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // Diagnostic: bypass wgpu compositor — pass decoded BGRA directly to encoder.
    let bypass_compositor = std::env::var("SGT_BYPASS_COMPOSITOR").is_ok();
    if bypass_compositor {
        println!("[Pipeline] BYPASS_COMPOSITOR mode — skipping wgpu, raw decode→encode");
    }

    // Pipelined readback: GPU renders frame N while CPU sets up frame N+1.
    // We queue a readback, move on, then drain it one frame later.
    let mut queued_readbacks: u32 = 0;

    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        t_wait += tw0.elapsed().as_secs_f64();

        if cancel_flag.load(Ordering::Relaxed) {
            // Return the receive buffer before exiting so decode thread doesn't stall.
            let _ = recycle_decode_tx.send(msg.bgra_video);
            break;
        }

        if bypass_compositor {
            if tx.send(RenderOutput { rendered_bgra: msg.bgra_video }).is_err() {
                break;
            }
            continue;
        }

        // 1. Upload video frame to GPU (synchronous — GPU copies immediately).
        let tu0 = Instant::now();
        compositor.upload_frame(&msg.bgra_video);
        t_upload += tu0.elapsed().as_secs_f64();

        // 2. Return the decoded video buffer to the decode thread — GPU has consumed it.
        let _ = recycle_decode_tx.send(msg.bgra_video);

        // 3. Submit render commands to GPU then composite atlas overlay quads if any.
        let tr0 = Instant::now();
        // Cursor-only fast path: when only the cursor is blurred (scene is sharp),
        // render the scene once and composite the cursor N times at 1/N opacity each.
        // This avoids N full scene re-renders while still blurring the cursor.
        let only_cursor_blur = mb_enabled && config.blur_cursor && !config.blur_zoom && !config.blur_pan;
        if only_cursor_blur {
            // Scene pass: render background + video with cursor at base_time, clear=true.
            let mut scene_u = build_uniforms(msg.source_time, msg.source_time, msg.source_time, msg.source_time);
            scene_u.render_mode = 1.0; // scene only, cursor skipped
            compositor.render_to_output(&scene_u, true);
            // Cursor blur passes: N renders at different sub-times, each at 1/N opacity.
            let half_shutter = step * config.motion_blur_shutter * 0.5;
            let opacity_scale = 1.0 / mb_samples as f32;
            for i in 0..mb_samples {
                let t = if mb_samples > 1 {
                    i as f64 / (mb_samples - 1) as f64
                } else {
                    0.5
                };
                let sub_time = msg.source_time - half_shutter + t * 2.0 * half_shutter;
                let mut cursor_u = build_uniforms(msg.source_time, msg.source_time, msg.source_time, sub_time);
                cursor_u.render_mode = 2.0; // cursor only; ALPHA_BLENDING composites over scene
                cursor_u.cursor_opacity *= opacity_scale;
                compositor.render_to_output(&cursor_u, false); // load existing scene
            }
        } else if mb_enabled {
            let half_shutter = step * config.motion_blur_shutter * 0.5;
            for i in 0..mb_samples {
                let t = if mb_samples > 1 {
                    i as f64 / (mb_samples - 1) as f64
                } else {
                    0.5
                };
                let sub_time = msg.source_time - half_shutter + t * 2.0 * half_shutter;
                // Isolate blur per channel: disabled channels sample base_time (sharp).
                let pan_time  = if config.blur_pan    { sub_time } else { msg.source_time };
                let zoom_time = if config.blur_zoom   { sub_time } else { msg.source_time };
                let cur_time  = if config.blur_cursor { sub_time } else { msg.source_time };
                let uniforms = build_uniforms(msg.source_time, pan_time, zoom_time, cur_time);
                let weight = 1.0 / (i as f64 + 1.0);
                compositor.render_accumulate(&uniforms, i == 0, weight);
            }
        } else {
            let uniforms = build_uniforms(msg.source_time, msg.source_time, msg.source_time, msg.source_time);
            compositor.render_to_output(&uniforms, true);
        }

        // Atlas overlay pass: draw GPU-accelerated quads on top of the rendered output.
        if let Some(frame) = config.overlay_frames.get(msg.frame_idx as usize) {
            compositor.render_overlays(&frame.quads);
        }

        // Enqueue GPU→CPU readback (async; drained below with pipeline depth 2).
        compositor.enqueue_output_readback()?;
        queued_readbacks += 1;

        t_render += tr0.elapsed().as_secs_f64();

        // 4. Pipelined readback: drain only if >= 2 frames are queued so the GPU
        //    has time to finish frame N while we set up frame N+1.
        if queued_readbacks >= 2 {
            let trb0 = Instant::now();
            let mut out_buf = recycle_render_rx
                .try_recv()
                .unwrap_or_default();
            compositor.readback_output(&mut out_buf)?;
            t_readback += trb0.elapsed().as_secs_f64();
            queued_readbacks -= 1;
            if tx.send(RenderOutput { rendered_bgra: out_buf }).is_err() {
                break;
            }
            frames_rendered += 1;
        }
    }

    // Drain all remaining GPU frames at end of video.
    while queued_readbacks > 0 {
        let trb0 = Instant::now();
        let mut out_buf = recycle_render_rx.try_recv().unwrap_or_default();
        compositor.readback_output(&mut out_buf)?;
        t_readback += trb0.elapsed().as_secs_f64();
        queued_readbacks -= 1;
        let _ = tx.send(RenderOutput { rendered_bgra: out_buf });
        frames_rendered += 1;
    }

    let n = frames_rendered.max(1) as f64;
    println!(
        "[Render] {} frames: upload {:.1} + render {:.1} + readback {:.1} + wait {:.1}ms avg",
        frames_rendered,
        t_upload / n * 1000.0,
        t_render / n * 1000.0,
        t_readback / n * 1000.0,
        t_wait / n * 1000.0,
    );

    Ok(())
}

/// Main thread: receives rendered BGRA frames and encodes to MP4.
fn run_encode_thread(
    config: &PipelineConfig,
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    rx: &mpsc::Receiver<RenderOutput>,
    total_frames: u32,
    start: &Instant,
    recycle_tx: mpsc::Sender<Vec<u8>>,
) -> Result<ZeroCopyExportResult, String> {
    // D3D11 device for encoder HW acceleration (main thread)
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

    let mut frames_encoded: u32 = 0;
    let mut t_encode = 0.0_f64;
    let mut t_wait = 0.0_f64;

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

        let te0 = Instant::now();
        let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;
        encoder.write_frame_cpu(&msg.rendered_bgra, timestamp_100ns, frame_duration_100ns)?;
        t_encode += te0.elapsed().as_secs_f64();

        // Return the output buffer to the render thread for reuse.
        let _ = recycle_tx.send(msg.rendered_bgra);

        frames_encoded += 1;

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

    encoder.finalize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frames_encoded as f64 / elapsed;
    let n = frames_encoded.max(1) as f64;

    println!(
        "[Encode] {} frames: encode {:.1} + wait {:.1}ms avg",
        frames_encoded,
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
