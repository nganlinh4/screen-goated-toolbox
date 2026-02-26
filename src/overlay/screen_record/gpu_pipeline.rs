// Threaded GPU export pipeline with zero-copy render→encode path.
//
// Three threads running in parallel:
//   Decode thread:  MF decode → D3D11 VP (NV12→BGRA) → CPU readback → channel
//   Render thread:  channel → wgpu upload → compositor render → [GPU copy | CPU readback] → channel
//   Main thread:    channel → MF encode → MP4
//
// Zero-copy output path (default):
//   wgpu renders → GPU copy to shared VRAM texture → MF encoder reads directly
//   Eliminates 2 PCIe crossings (~4GB/s at 4K60). Ring buffer of 3 shared slots.
//
// CPU fallback (env SGT_FORCE_CPU_ENCODE=1 or if shared texture init fails):
//   wgpu renders → GPU→CPU readback → CPU→MF encoder (original path)
//
// Frame selection: sample-and-hold using source PTS to handle VFR sources.
// wgpu (DX12) and D3D11 use completely independent devices — no D3D11On12.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;

use windows::core::Interface;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

use super::d3d_interop::{create_d3d11_device, D3D11Readback, SharedVramBuffer, VideoProcessor};
use super::gpu_export::{CompositorUniforms, GpuCompositor};
use super::mf_audio::{AudioConfig, MfAudioDecoder};
use super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::mf_encode::{EncoderConfig, MfEncoder, VideoCodec};
use super::native_export::config::{OverlayFrame, SpeedPoint, TrimSegment};

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
    pub audio_path: Option<String>,
    /// Volume multiplier applied to every PCM sample (0.0 = silent, 1.0 = unchanged).
    pub audio_volume: f64,
    pub output_width: u32,
    pub output_height: u32,
    pub framerate: u32,
    pub bitrate_kbps: u32,
    pub speed_points: Vec<SpeedPoint>,
    pub trim_start: f64,
    pub duration: f64,
    pub codec: VideoCodec,
    pub trim_segments: Vec<TrimSegment>,
    pub motion_blur_samples: u32,
    pub blur_zoom_shutter: f64,
    pub blur_pan_shutter: f64,
    pub blur_cursor_shutter: f64,
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
    source_step: f64,
    frame_idx: u32,
}

/// Message sent from render thread to encode thread.
enum RenderOutput {
    /// CPU path: rendered BGRA pixels (out_w×h×4). Returned to render via recycle.
    Cpu { rendered_bgra: Vec<u8> },
    /// GPU path: index into shared VRAM ring buffer. Returned to render via recycle.
    Gpu { ring_idx: usize },
}

const GPU_RING_SIZE: usize = 3;

/// Shared VRAM ring for zero-copy render→encode.
struct GpuOutputRing {
    shared_buffers: Vec<SharedVramBuffer>,
    wgpu_textures: Vec<wgpu::Texture>,
}

fn get_speed(time: f64, points: &[SpeedPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|p| p.time < time);
    if idx == 0 {
        return points[0].speed;
    }
    if idx >= points.len() {
        return points.last().unwrap().speed;
    }

    let p1 = &points[idx - 1];
    let p2 = &points[idx];
    let t = (time - p1.time) / (p2.time - p1.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    p1.speed + (p2.speed - p1.speed) * cos_t
}

pub fn build_frame_times(config: &PipelineConfig) -> Vec<f64> {
    let mut times = Vec::new();
    let out_dt = 1.0 / config.framerate as f64;

    let trim_segments = if config.trim_segments.is_empty() {
        vec![TrimSegment {
            start_time: config.trim_start,
            end_time: config.trim_start + config.duration,
        }]
    } else {
        config.trim_segments.clone()
    };

    if trim_segments.is_empty() {
        return times;
    }

    let mut seg_idx = 0usize;
    let mut current_source_time = trim_segments[0].start_time;
    let end_time = trim_segments.last().unwrap().end_time;

    while current_source_time < end_time - 1e-9 {
        while seg_idx < trim_segments.len()
            && current_source_time >= trim_segments[seg_idx].end_time
        {
            seg_idx += 1;
            if seg_idx < trim_segments.len() {
                current_source_time = trim_segments[seg_idx].start_time;
            }
        }
        if seg_idx >= trim_segments.len() {
            break;
        }

        times.push(current_source_time);
        let speed = get_speed(current_source_time, &config.speed_points).clamp(0.1, 16.0);
        current_source_time += speed * out_dt;
    }

    times
}

/// Import a shared D3D11 texture (NT handle) into wgpu as a DX12 texture.
///
/// Bridges windows 0.62 (our crate) ↔ windows 0.58 (wgpu-hal) by reinterpreting
/// COM pointers. Both versions are ABI-identical `#[repr(transparent)]` wrappers.
unsafe fn import_shared_handle_into_wgpu(
    device: &wgpu::Device,
    handle: HANDLE,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    use windows::Win32::Graphics::Direct3D12 as d3d12;

    let hal_texture =
        device.as_hal::<wgpu::hal::api::Dx12, _, _>(|hal_dev| -> Result<_, String> {
            let hal_dev = hal_dev.ok_or("No DX12 HAL device")?;

            // wgpu-hal's raw_device() returns &windows_058::ID3D12Device.
            // Reinterpret as our windows 0.62 type — same COM vtable, same ABI.
            let hal_d12_ref = hal_dev.raw_device();
            let our_d12: &d3d12::ID3D12Device =
                &*(hal_d12_ref as *const _ as *const d3d12::ID3D12Device);

            // Open the shared NT handle → D3D12 resource (windows 0.62).
            let mut d3d12_resource: Option<d3d12::ID3D12Resource> = None;
            our_d12
                .OpenSharedHandle(handle, &mut d3d12_resource)
                .map_err(|e| format!("OpenSharedHandle: {e}"))?;
            let d3d12_resource =
                d3d12_resource.ok_or_else(|| "OpenSharedHandle returned null".to_string())?;

            // Convert 0.62 ID3D12Resource → 0.58 for texture_from_raw.
            // Both are pointer-width COM wrappers — bitwise identical.
            let hal_resource = std::mem::transmute_copy(&d3d12_resource);
            std::mem::forget(d3d12_resource); // ownership transferred, prevent double-Release

            Ok(wgpu::hal::dx12::Device::texture_from_raw(
                hal_resource,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                wgpu::TextureDimension::D2,
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                1,
                1,
            ))
        })?;

    let desc = wgpu::TextureDescriptor {
        label: Some("Shared Output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    };

    Ok(device.create_texture_from_hal::<wgpu::hal::api::Dx12>(hal_texture, &desc))
}

/// Try to create a GPU output ring (shared VRAM textures imported into wgpu).
/// Returns None if any step fails — caller should fall back to CPU path.
fn try_create_gpu_output_ring(
    enc_device: &ID3D11Device,
    wgpu_device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Option<GpuOutputRing> {
    let mut shared_buffers = Vec::with_capacity(GPU_RING_SIZE);
    let mut wgpu_textures = Vec::with_capacity(GPU_RING_SIZE);

    for i in 0..GPU_RING_SIZE {
        let buf = match SharedVramBuffer::new(enc_device, width, height) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[Export] SharedVramBuffer[{i}] failed: {e}");
                return None;
            }
        };
        let tex = match unsafe {
            import_shared_handle_into_wgpu(wgpu_device, buf.handle, width, height)
        } {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[Export] wgpu import[{i}] failed: {e}");
                return None;
            }
        };
        shared_buffers.push(buf);
        wgpu_textures.push(tex);
    }

    Some(GpuOutputRing {
        shared_buffers,
        wgpu_textures,
    })
}

/// Run the threaded GPU export pipeline.
pub fn run_zero_copy_export(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &(dyn Fn(f64, f64, f64, f64) -> CompositorUniforms + Sync),
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    source_times: &[f64],
) -> Result<ZeroCopyExportResult, String> {
    let start = Instant::now();
    let total_frames = source_times.len() as u32;
    let mb_samples = config.motion_blur_samples.max(1);
    let mb_enabled = mb_samples > 1
        && (config.blur_zoom_shutter > 0.0
            || config.blur_pan_shutter > 0.0
            || config.blur_cursor_shutter > 0.0);

    // Create encode D3D11 device BEFORE thread::scope so shared textures
    // can be created and imported into wgpu for zero-copy output.
    let (enc_device, _enc_context) = create_d3d11_device()?;
    {
        let mt: ID3D11Multithread = enc_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (enc): {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }
    let enc_device_manager = DxgiDeviceManager::new(&enc_device)?;

    // Try zero-copy GPU output ring (shared VRAM textures).
    let force_cpu = std::env::var("SGT_FORCE_CPU_ENCODE").is_ok();
    let gpu_ring = if force_cpu {
        println!("[Export] SGT_FORCE_CPU_ENCODE set, using CPU output path");
        None
    } else {
        match try_create_gpu_output_ring(
            &enc_device,
            compositor.device(),
            config.output_width,
            config.output_height,
        ) {
            Some(ring) => {
                println!(
                    "[Export] Using zero-copy GPU output path ({}-slot ring)",
                    GPU_RING_SIZE
                );
                Some(ring)
            }
            None => {
                println!("[Export] Falling back to CPU output path");
                None
            }
        }
    };
    let use_gpu = gpu_ring.is_some();

    // Borrow ring contents for the scoped threads.
    let gpu_wgpu_textures: &[wgpu::Texture] = gpu_ring
        .as_ref()
        .map(|r| r.wgpu_textures.as_slice())
        .unwrap_or(&[]);
    let gpu_shared_buffers: &[SharedVramBuffer] = gpu_ring
        .as_ref()
        .map(|r| r.shared_buffers.as_slice())
        .unwrap_or(&[]);

    // Forward channels (decode → render → encode).
    let (decode_tx, render_rx) = mpsc::sync_channel::<DecodeOutput>(3);
    let (render_tx, encode_rx) = mpsc::sync_channel::<RenderOutput>(3);
    // Recycle channels (backwards): buffers return to their producer thread for reuse.
    let (render_to_decode_tx, render_to_decode_rx) = mpsc::channel::<Vec<u8>>();
    // CPU output recycle (only active when gpu_ring is None).
    let (cpu_recycle_tx, cpu_recycle_rx) = mpsc::channel::<Vec<u8>>();
    // GPU slot recycle (only active when gpu_ring is Some).
    let (gpu_recycle_tx, gpu_recycle_rx) = mpsc::channel::<usize>();
    if use_gpu {
        for i in 0..GPU_RING_SIZE {
            let _ = gpu_recycle_tx.send(i);
        }
    }

    let decode_error: std::sync::Arc<Mutex<Option<String>>> = std::sync::Arc::new(Mutex::new(None));
    let render_error: std::sync::Arc<Mutex<Option<String>>> = std::sync::Arc::new(Mutex::new(None));

    let mut result: Result<ZeroCopyExportResult, String> = Err("pipeline did not run".into());
    let decode_err_clone = decode_error.clone();
    let render_err_clone = render_error.clone();

    println!(
        "[Pipeline] {} frames, {}x{} → {}x{} @ {}fps, blur={} (z={:.2}, p={:.2}, c={:.2}, mb={}), segs={}",
        total_frames,
        config.video_width,
        config.video_height,
        config.output_width,
        config.output_height,
        config.framerate,
        config.motion_blur_samples,
        config.blur_zoom_shutter,
        config.blur_pan_shutter,
        config.blur_cursor_shutter,
        mb_enabled,
        config.trim_segments.len()
    );

    std::thread::scope(|s| {
        // Thread 1: Decode
        s.spawn(move || {
            if let Err(e) = run_decode_thread(
                config,
                cancel_flag,
                source_times,
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
                mb_samples,
                render_tx,
                render_to_decode_tx,
                gpu_wgpu_textures,
                gpu_recycle_rx,
                cpu_recycle_rx,
            ) {
                cancel_flag.store(true, Ordering::Relaxed);
                *render_err_clone.lock().unwrap() = Some(e);
            }
        });

        // Main thread: Encode
        result = run_encode_thread(
            config,
            &enc_device_manager,
            progress,
            cancel_flag,
            &encode_rx,
            total_frames,
            &start,
            gpu_shared_buffers,
            gpu_recycle_tx,
            cpu_recycle_tx,
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
    source_times: &[f64],
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

    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;
    let mut src_decoded: u32 = if have_next { 2 } else { 1 };

    for (frame_idx, &source_time) in source_times.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let next_source_time = source_times
            .get(frame_idx + 1)
            .copied()
            .unwrap_or(source_time);
        let mut source_step = next_source_time - source_time;
        if source_step <= 0.0 {
            let speed = get_speed(source_time, &config.speed_points).clamp(0.1, 16.0);
            source_step = speed / config.framerate as f64;
        }

        // Seek on trim segment boundary change.
        if !config.trim_segments.is_empty() {
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

        if tx
            .send(DecodeOutput {
                bgra_video: send_buf,
                source_time,
                source_step,
                frame_idx: frame_idx as u32,
            })
            .is_err()
        {
            break;
        }
    }

    let elapsed = t_thread.elapsed().as_secs_f64();
    println!(
        "[Decode] {} src → {} out ({} held) in {:.1}s",
        src_decoded,
        source_times.len(),
        frames_held,
        elapsed
    );
    Ok(())
}

/// Render thread: receives BGRA frames, runs compositor, sends output to encoder.
///
/// GPU path: copy output to shared VRAM texture, send ring_idx (no CPU readback).
/// CPU path: pipelined readback depth 2, send BGRA Vec.
#[allow(clippy::too_many_arguments)]
fn run_render_thread(
    config: &PipelineConfig,
    compositor: &mut GpuCompositor,
    build_uniforms: &(dyn Fn(f64, f64, f64, f64) -> CompositorUniforms + Sync),
    cancel_flag: &std::sync::atomic::AtomicBool,
    rx: mpsc::Receiver<DecodeOutput>,
    mb_samples: u32,
    tx: mpsc::SyncSender<RenderOutput>,
    recycle_decode_tx: mpsc::Sender<Vec<u8>>,
    gpu_textures: &[wgpu::Texture],
    gpu_slot_rx: mpsc::Receiver<usize>,
    cpu_recycle_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), String> {
    let use_gpu = !gpu_textures.is_empty();
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

    // Pipelined readback state (CPU path only).
    let mut queued_readbacks: u32 = 0;

    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        t_wait += tw0.elapsed().as_secs_f64();

        if cancel_flag.load(Ordering::Relaxed) {
            let _ = recycle_decode_tx.send(msg.bgra_video);
            break;
        }

        if bypass_compositor {
            if tx
                .send(RenderOutput::Cpu {
                    rendered_bgra: msg.bgra_video,
                })
                .is_err()
            {
                break;
            }
            continue;
        }

        // 1. Upload video frame to GPU.
        let tu0 = Instant::now();
        compositor.upload_frame(&msg.bgra_video);
        t_upload += tu0.elapsed().as_secs_f64();

        // 2. Return the decoded video buffer to the decode thread.
        let _ = recycle_decode_tx.send(msg.bgra_video);

        // 3. Render to output texture (same for both paths).
        let tr0 = Instant::now();
        let only_cursor_blur = mb_samples > 1
            && config.blur_cursor_shutter > 0.0
            && config.blur_zoom_shutter == 0.0
            && config.blur_pan_shutter == 0.0;
        if only_cursor_blur {
            let mut scene_u = build_uniforms(
                msg.source_time,
                msg.source_time,
                msg.source_time,
                msg.source_time,
            );
            scene_u.render_mode = 1.0;
            compositor.render_to_output(&scene_u, true);
            let opacity_scale = 1.0 / mb_samples as f32;
            let cursor_shutter = msg.source_step * config.blur_cursor_shutter;
            for i in 0..mb_samples {
                let t = if mb_samples > 1 {
                    i as f64 / (mb_samples - 1) as f64
                } else {
                    0.5
                };
                let sub_time = msg.source_time - (cursor_shutter * 0.5) + t * cursor_shutter;
                let mut cursor_u =
                    build_uniforms(msg.source_time, msg.source_time, msg.source_time, sub_time);
                cursor_u.render_mode = 2.0;
                cursor_u.cursor_opacity *= opacity_scale;
                compositor.render_to_output(&cursor_u, false);
            }
        } else if mb_samples > 1 {
            let zoom_shutter = msg.source_step * config.blur_zoom_shutter;
            let pan_shutter = msg.source_step * config.blur_pan_shutter;
            let cursor_shutter = msg.source_step * config.blur_cursor_shutter;
            for i in 0..mb_samples {
                let t = if mb_samples > 1 {
                    i as f64 / (mb_samples - 1) as f64
                } else {
                    0.5
                };
                let pan_time = msg.source_time - (pan_shutter * 0.5) + t * pan_shutter;
                let zoom_time = msg.source_time - (zoom_shutter * 0.5) + t * zoom_shutter;
                let cur_time = msg.source_time - (cursor_shutter * 0.5) + t * cursor_shutter;
                let uniforms = build_uniforms(msg.source_time, pan_time, zoom_time, cur_time);
                let weight = 1.0 / (i as f64 + 1.0);
                compositor.render_accumulate(&uniforms, i == 0, weight);
            }
        } else {
            let uniforms = build_uniforms(
                msg.source_time,
                msg.source_time,
                msg.source_time,
                msg.source_time,
            );
            compositor.render_to_output(&uniforms, true);
        }

        // Atlas overlay pass.
        if let Some(frame) = config.overlay_frames.get(msg.frame_idx as usize) {
            compositor.render_overlays(&frame.quads);
        }

        t_render += tr0.elapsed().as_secs_f64();

        // 4. Output: GPU zero-copy or CPU readback.
        if use_gpu {
            // GPU path: copy to shared VRAM texture, wait for GPU, send ring_idx.
            let ring_idx = gpu_slot_rx
                .recv()
                .map_err(|_| "GPU slot recycle channel closed")?;
            let trb0 = Instant::now();
            compositor.copy_output_to_shared(&gpu_textures[ring_idx]);
            // Wait for DX12 copy to complete before D3D11 reads the shared texture.
            let _ = compositor.device().poll(wgpu::PollType::Wait);
            t_readback += trb0.elapsed().as_secs_f64();
            if tx.send(RenderOutput::Gpu { ring_idx }).is_err() {
                break;
            }
            frames_rendered += 1;
        } else {
            // CPU path: pipelined readback (depth 2).
            compositor.enqueue_output_readback()?;
            queued_readbacks += 1;
            if queued_readbacks >= 2 {
                let trb0 = Instant::now();
                let mut out_buf = cpu_recycle_rx.try_recv().unwrap_or_default();
                compositor.readback_output(&mut out_buf)?;
                t_readback += trb0.elapsed().as_secs_f64();
                queued_readbacks -= 1;
                if tx
                    .send(RenderOutput::Cpu {
                        rendered_bgra: out_buf,
                    })
                    .is_err()
                {
                    break;
                }
                frames_rendered += 1;
            }
        }
    }

    // CPU path: drain remaining GPU readbacks at end of video.
    while queued_readbacks > 0 {
        let trb0 = Instant::now();
        let mut out_buf = cpu_recycle_rx.try_recv().unwrap_or_default();
        compositor.readback_output(&mut out_buf)?;
        t_readback += trb0.elapsed().as_secs_f64();
        queued_readbacks -= 1;
        let _ = tx.send(RenderOutput::Cpu {
            rendered_bgra: out_buf,
        });
        frames_rendered += 1;
    }

    let n = frames_rendered.max(1) as f64;
    let label = if use_gpu { "copy" } else { "readback" };
    println!(
        "[Render] {} frames: upload {:.1} + render {:.1} + {} {:.1} + wait {:.1}ms avg",
        frames_rendered,
        t_upload / n * 1000.0,
        t_render / n * 1000.0,
        label,
        t_readback / n * 1000.0,
        t_wait / n * 1000.0,
    );

    Ok(())
}

/// Fast linear interpolation for native audio speed alteration (pitch-shifts).
fn resample_pcm_bytes(input: &[u8], speed: f64, channels: usize) -> Vec<u8> {
    if (speed - 1.0).abs() < 0.001 || input.is_empty() || channels == 0 {
        return input.to_vec();
    }
    if !input.len().is_multiple_of(4) {
        return input.to_vec();
    }

    let samples = input.len() / 4;
    if samples < channels * 2 {
        return input.to_vec();
    }

    let mut input_f32 = vec![0.0f32; samples];
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.as_ptr(),
            input_f32.as_mut_ptr() as *mut u8,
            input.len(),
        );
    }

    let in_frames = input_f32.len() / channels;
    if in_frames < 2 {
        return input.to_vec();
    }
    let out_frames = ((in_frames as f64) / speed).max(1.0) as usize;
    let mut output_f32 = Vec::with_capacity(out_frames * channels);

    for i in 0..out_frames {
        let src_idx = i as f64 * speed;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(in_frames - 1);
        let frac = (src_idx - idx0 as f64) as f32;
        for c in 0..channels {
            let v0 = input_f32[idx0 * channels + c];
            let v1 = input_f32[idx1 * channels + c];
            output_f32.push(v0 + (v1 - v0) * frac);
        }
    }

    let out_bytes = output_f32.len() * 4;
    let mut output_u8 = vec![0u8; out_bytes];
    unsafe {
        std::ptr::copy_nonoverlapping(
            output_f32.as_ptr() as *const u8,
            output_u8.as_mut_ptr(),
            out_bytes,
        );
    }
    output_u8
}

/// Main thread: receives rendered frames, interleaves audio, and encodes to MP4.
///
/// GPU path: reads directly from shared VRAM textures via MFCreateDXGISurfaceBuffer.
/// CPU path: receives BGRA Vec and uses MFCreateMemoryBuffer (original path).
#[allow(clippy::too_many_arguments)]
fn run_encode_thread(
    config: &PipelineConfig,
    enc_device_manager: &DxgiDeviceManager,
    progress: Option<ProgressCallback>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    rx: &mpsc::Receiver<RenderOutput>,
    total_frames: u32,
    start: &Instant,
    gpu_buffers: &[SharedVramBuffer],
    gpu_recycle_tx: mpsc::Sender<usize>,
    cpu_recycle_tx: mpsc::Sender<Vec<u8>>,
) -> Result<ZeroCopyExportResult, String> {
    let encoder_config = EncoderConfig {
        codec: config.codec,
        width: config.output_width,
        height: config.output_height,
        fps_num: config.framerate,
        fps_den: 1,
        bitrate_kbps: config.bitrate_kbps,
    };
    let mut audio_decoder = None;
    let mut audio_config = None;

    if let Some(path) = &config.audio_path {
        if !path.is_empty() {
            match MfAudioDecoder::new(path) {
                Ok(dec) => {
                    audio_config = Some(AudioConfig {
                        sample_rate: dec.sample_rate(),
                        channels: dec.channels(),
                        bitrate_kbps: 192,
                    });
                    audio_decoder = Some(dec);
                }
                Err(e) => eprintln!("[Audio] Failed to open native audio decoder: {}", e),
            }
        }
    }

    let (encoder, opt_audio_stream) = MfEncoder::new(
        &config.output_path,
        encoder_config,
        enc_device_manager,
        audio_config.as_ref(),
    )?;
    let frame_duration_100ns = encoder.frame_duration_100ns();

    let mut audio_output_100ns = 0i64;
    let mut audio_segment_idx = 0usize;
    let mut audio_eof = false;
    let mut total_audio_samples_written: u64 = 0;

    if let Some(dec) = &audio_decoder {
        let start_time = if config.trim_segments.is_empty() {
            config.trim_start
        } else {
            config.trim_segments[0].start_time
        };
        if start_time > 0.0 {
            let _ = dec.seek((start_time * 10_000_000.0) as i64);
        }
    }

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

        let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;

        // Audio interleaving (identical for both paths).
        if let (Some(dec), Some(stream)) = (&audio_decoder, &opt_audio_stream) {
            while !audio_eof && audio_output_100ns <= timestamp_100ns {
                let current_seg = if config.trim_segments.is_empty() {
                    Some((config.trim_start, config.trim_start + config.duration))
                } else {
                    config
                        .trim_segments
                        .get(audio_segment_idx)
                        .map(|s| (s.start_time, s.end_time))
                };
                let Some((seg_start, seg_end)) = current_seg else {
                    audio_eof = true;
                    break;
                };

                match dec.read_samples() {
                    Ok(Some((pcm, ts_100ns))) => {
                        let chunk_time = ts_100ns as f64 / 10_000_000.0;
                        if chunk_time > seg_end {
                            audio_segment_idx += 1;
                            if config.trim_segments.is_empty() {
                                audio_eof = true;
                            } else if let Some(next_seg) =
                                config.trim_segments.get(audio_segment_idx)
                            {
                                let _ = dec.seek((next_seg.start_time * 10_000_000.0) as i64);
                            }
                            continue;
                        }
                        if chunk_time < seg_start {
                            continue;
                        }
                        if chunk_time <= seg_end {
                            let channels = dec.channels() as usize;
                            let speed =
                                get_speed(chunk_time, &config.speed_points).clamp(0.1, 16.0);
                            let mut resampled = resample_pcm_bytes(&pcm, speed, channels);
                            // Apply volume scaling (config.audio_volume == 1.0 → no-op).
                            if config.audio_volume < 0.999 {
                                let vol = config.audio_volume as f32;
                                for chunk in resampled.chunks_exact_mut(4) {
                                    let s = f32::from_le_bytes(chunk.try_into().unwrap());
                                    chunk.copy_from_slice(&(s * vol).clamp(-1.0, 1.0).to_le_bytes());
                                }
                            }
                            if channels == 0 || resampled.is_empty() {
                                continue;
                            }
                            let samples_per_channel = resampled.len() / (channels * 4);
                            if samples_per_channel == 0 {
                                continue;
                            }
                            let next_total =
                                total_audio_samples_written + samples_per_channel as u64;
                            let next_100ns =
                                (next_total * 10_000_000) / dec.sample_rate() as u64;
                            let dur_100ns = next_100ns as i64 - audio_output_100ns;
                            if dur_100ns <= 0 {
                                continue;
                            }
                            if let Err(e) = stream.write_samples_direct(
                                encoder.writer(),
                                &resampled,
                                audio_output_100ns,
                                dur_100ns,
                            ) {
                                eprintln!("[Audio] Native audio write error: {}", e);
                                audio_eof = true;
                            } else {
                                total_audio_samples_written = next_total;
                                audio_output_100ns = next_100ns as i64;
                            }
                        }
                    }
                    Ok(None) => audio_eof = true,
                    Err(e) => {
                        eprintln!("[Audio] Native audio decode error: {}", e);
                        audio_eof = true;
                    }
                }
            }
        }

        // Video encode: GPU or CPU path based on message variant.
        let te0 = Instant::now();
        match msg {
            RenderOutput::Gpu { ring_idx } => {
                encoder.write_frame_gpu(
                    &gpu_buffers[ring_idx].texture,
                    timestamp_100ns,
                    frame_duration_100ns,
                )?;
                // Return the ring slot to the render thread for reuse.
                let _ = gpu_recycle_tx.send(ring_idx);
            }
            RenderOutput::Cpu { rendered_bgra } => {
                encoder.write_frame_cpu(&rendered_bgra, timestamp_100ns, frame_duration_100ns)?;
                let _ = cpu_recycle_tx.send(rendered_bgra);
            }
        }
        t_encode += te0.elapsed().as_secs_f64();

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
