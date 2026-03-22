// Threaded GPU export pipeline with fully zero-copy decode→render→encode path.
//
// Three threads running in parallel:
//   Decode thread:  MF decode → D3D11 VP (NV12→BGRA) → shared VRAM texture → channel
//   Render thread:  channel → GPU copy to video texture → compositor render → GPU copy to shared → channel
//   Main thread:    channel → MF encode → MP4
//
// Zero-copy path (default):
//   Decode: D3D11 VP blits directly into shared VRAM texture (NT handle), GPU fence, send ring index.
//   Render: wgpu copies shared decode texture to video_texture (GPU-to-GPU), renders, copies output
//           to shared encode texture. No PCIe bus crossings in the entire pipeline.
//   Encode: MF encoder reads directly from shared VRAM via MFCreateDXGISurfaceBuffer.
//
// CPU fallback (env SGT_FORCE_CPU_ENCODE=1 or if shared texture init fails):
//   Decode: D3D11 VP → CPU readback → channel (Vec<u8>)
//   Render: CPU upload → compositor → [GPU copy | CPU readback] → channel
//
// Frame selection: sample-and-hold using source PTS to handle VFR sources.
// wgpu (DX12) and D3D11 use completely independent devices — no D3D11On12.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

use windows::Win32::Foundation::GENERIC_ALL;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12 as d3d12;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::Interface;

use super::d3d_interop::{
    D3D11GpuFence, D3D11Readback, SharedVramBuffer, VideoProcessor, create_d3d11_device,
    create_d3d11_device_on_adapter,
};
use super::gpu_export::{CompositorUniforms, GpuCompositor};
use super::mf_audio::{AudioConfig, MfAudioDecoder};
use super::mf_decode::{DecodedFrame, DxgiDeviceManager, MfDecoder};
use super::mf_encode::{EncoderConfig, MfEncoder, VideoCodec};
use super::native_export::config::{
    AnimatedCursorSlotData, BakedWebcamFrame, DeviceAudioPoint, OverlayFrame, SpeedPoint,
    TrimSegment,
};

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
    pub audio_is_preprocessed: bool,
    /// Device audio volume curve in source-video time (0.0 = silent, 1.0 = unchanged).
    pub audio_volume_points: Vec<DeviceAudioPoint>,
    pub webcam_video_path: Option<String>,
    pub webcam_offset_sec: f64,
    pub webcam_frames: Vec<BakedWebcamFrame>,
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
    /// Pre-rasterized animation frames for animated cursor atlas slots.
    /// Each entry is updated in the atlas before every output frame render.
    pub animated_cursor_slots: Vec<AnimatedCursorSlotData>,
}

/// Message sent from decode thread to render thread.
enum DecodeOutput {
    /// GPU path: index into shared decode VRAM ring. Returned to decode via recycle.
    Gpu {
        ring_idx: usize,
        source_time: f64,
        source_step: f64,
        frame_idx: u32,
        /// Cross-API fence value signaled by D3D11 after writing this frame.
        /// The render thread must Wait on the DX12 queue for this value before reading.
        fence_value: u64,
    },
    /// GPU path hold frame: source frame did not advance, reuse the previous video texture.
    GpuHold {
        source_time: f64,
        source_step: f64,
        frame_idx: u32,
    },
    /// CPU fallback: BGRA pixels (video_w×h×4). Returned to decode via recycle.
    Cpu {
        bgra_video: Vec<u8>,
        source_time: f64,
        source_step: f64,
        frame_idx: u32,
    },
    /// Stream inactive for this frame (e.g. webcam fully hidden); no GPU work needed.
    Inactive {
        source_time: f64,
        source_step: f64,
        frame_idx: u32,
    },
}

impl DecodeOutput {
    fn source_time(&self) -> f64 {
        match self {
            Self::Gpu { source_time, .. }
            | Self::GpuHold { source_time, .. }
            | Self::Cpu { source_time, .. }
            | Self::Inactive { source_time, .. } => *source_time,
        }
    }
    fn source_step(&self) -> f64 {
        match self {
            Self::Gpu { source_step, .. }
            | Self::GpuHold { source_step, .. }
            | Self::Cpu { source_step, .. }
            | Self::Inactive { source_step, .. } => *source_step,
        }
    }
    fn frame_idx(&self) -> u32 {
        match self {
            Self::Gpu { frame_idx, .. }
            | Self::GpuHold { frame_idx, .. }
            | Self::Cpu { frame_idx, .. }
            | Self::Inactive { frame_idx, .. } => *frame_idx,
        }
    }
}

/// Message sent from render thread to encode thread.
enum RenderOutput {
    /// CPU path: rendered BGRA pixels (out_w×h×4). Returned to render via recycle.
    Cpu { rendered_bgra: Vec<u8> },
    /// GPU path: index into shared VRAM ring buffer. Returned to render via recycle.
    Gpu { ring_idx: usize },
}

const GPU_RING_SIZE: usize = 16;
const DECODE_RING_SIZE: usize = 3;

/// Shared VRAM ring for zero-copy render→encode.
struct GpuOutputRing {
    shared_buffers: Vec<SharedVramBuffer>,
    wgpu_textures: Vec<wgpu::Texture>,
    /// Keyed mutex handles for GPU cache coherence between DX12 (render) and D3D11 (encode).
    /// Render thread: AcquireSync before copy, ReleaseSync after poll(Wait).
    /// Encode thread QIs its own set from the same textures.
    dx12_keyed_mutexes: Vec<IDXGIKeyedMutex>,
}

/// Shared VRAM ring for zero-copy decode→render.
///
/// Uses D3D11-created `SHARED_KEYEDMUTEX | SHARED_NTHANDLE` textures.
/// Keyed mutex provides CPU-level ownership. Cross-API shared fence provides
/// GPU-timeline ordering (D3D11 Signal → DX12 Wait). A per-frame 1-pixel
/// `copy_buffer_to_texture` (COPY_DST) before `copy_texture_to_texture` (COPY_SRC)
/// forces a COPY_DST→COPY_SRC DX12 barrier that flushes L2 cache.
///
/// DX12 can't participate in keyed mutex cache coherence (QI for IDXGIKeyedMutex
/// from ID3D12Resource returns E_NOINTERFACE), so we manually force a DX12 barrier
/// via the COPY_DST→COPY_SRC state transition trick.
struct DecodeInputRing {
    shared_buffers: Vec<SharedVramBuffer>,
    wgpu_textures: Vec<wgpu::Texture>,
    keyed_mutexes: Vec<IDXGIKeyedMutex>,
    d3d11_fence: ID3D11Fence,
    d3d12_fence: d3d12::ID3D12Fence,
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

fn get_device_audio_volume(time: f64, points: &[DeviceAudioPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|p| p.time < time);
    if idx == 0 {
        return points[0].volume.clamp(0.0, 1.0);
    }
    if idx >= points.len() {
        return points.last().unwrap().volume.clamp(0.0, 1.0);
    }

    let p1 = &points[idx - 1];
    let p2 = &points[idx];
    let t = (time - p1.time) / (p2.time - p1.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    (p1.volume + (p2.volume - p1.volume) * cos_t).clamp(0.0, 1.0)
}

fn apply_audio_volume_envelope(
    pcm: &mut [u8],
    source_start_time: f64,
    source_duration_sec: f64,
    channels: usize,
    points: &[DeviceAudioPoint],
) {
    if pcm.is_empty() || channels == 0 {
        return;
    }

    let frames = pcm.len() / (channels * 4);
    if frames == 0 {
        return;
    }

    if points
        .iter()
        .all(|point| (point.volume.clamp(0.0, 1.0) - 1.0).abs() < 0.0001)
    {
        return;
    }

    if let Some(first_point) = points.first() {
        let constant_volume = first_point.volume.clamp(0.0, 1.0) as f32;
        if points
            .iter()
            .all(|point| (point.volume.clamp(0.0, 1.0) - constant_volume as f64).abs() < 0.0001)
        {
            for chunk in pcm.chunks_exact_mut(4) {
                let sample = f32::from_le_bytes(chunk.try_into().unwrap());
                chunk.copy_from_slice(&(sample * constant_volume).clamp(-1.0, 1.0).to_le_bytes());
            }
            return;
        }
    }

    let frame_time_step = if source_duration_sec <= 0.0 {
        0.0
    } else {
        source_duration_sec / frames as f64
    };

    for frame_idx in 0..frames {
        let sample_time = source_start_time + ((frame_idx as f64) + 0.5) * frame_time_step;
        let volume = get_device_audio_volume(sample_time, points) as f32;
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for channel_idx in 0..channels {
            let sample_idx = ((frame_idx * channels) + channel_idx) * 4;
            let sample = f32::from_le_bytes(pcm[sample_idx..sample_idx + 4].try_into().unwrap());
            pcm[sample_idx..sample_idx + 4]
                .copy_from_slice(&(sample * volume).clamp(-1.0, 1.0).to_le_bytes());
        }
    }
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

struct WebcamDecodeSetup {
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    ring: DecodeInputRing,
    source_times: Vec<f64>,
    active_mask: Vec<bool>,
    source_width: u32,
    source_height: u32,
    render_width: u32,
    render_height: u32,
}

fn prepare_webcam_decode_setup(
    source_times: &[f64],
    config: &PipelineConfig,
    wgpu_vendor: u32,
    wgpu_device_id: u32,
    wgpu_device: &wgpu::Device,
) -> Result<Option<WebcamDecodeSetup>, String> {
    let Some(path) = config
        .webcam_video_path
        .as_ref()
        .filter(|path| !path.trim().is_empty())
    else {
        return Ok(None);
    };
    if !std::path::Path::new(path).exists() || config.webcam_frames.is_empty() {
        return Ok(None);
    }
    if config.webcam_frames.len() != source_times.len() {
        return Err(format!(
            "Webcam baked frames length {} does not match export frames {}",
            config.webcam_frames.len(),
            source_times.len()
        ));
    }

    let active_mask: Vec<bool> = config
        .webcam_frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let webcam_media_time = source_times[index] - config.webcam_offset_sec;
            webcam_media_time >= 0.0
                && frame.visible
                && frame.opacity > 0.001
                && frame.width > 0.0
                && frame.height > 0.0
        })
        .collect();
    if !active_mask.iter().any(|active| *active) {
        return Ok(None);
    }

    let mut max_width = 0.0f64;
    let mut max_height = 0.0f64;
    for frame in &config.webcam_frames {
        if !(frame.visible && frame.opacity > 0.001) {
            continue;
        }
        max_width = max_width.max(frame.width.max(0.0));
        max_height = max_height.max(frame.height.max(0.0));
    }
    if max_width <= 0.0 || max_height <= 0.0 {
        return Ok(None);
    }

    let (d3d_device, d3d_context) = if wgpu_vendor != 0 {
        create_d3d11_device_on_adapter(wgpu_vendor, wgpu_device_id)?
    } else {
        create_d3d11_device()?
    };
    {
        let mt: ID3D11Multithread = d3d_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (webcam): {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }
    let device_manager = DxgiDeviceManager::new(&d3d_device)?;
    let decoder = MfDecoder::new(path, &device_manager, true)?;
    let source_width = decoder.width();
    let source_height = decoder.height();
    let render_width = (max_width.ceil() as u32).clamp(2, decoder.width().max(2));
    let render_height = (max_height.ceil() as u32).clamp(2, decoder.height().max(2));
    drop(decoder);

    let ring = try_create_decode_input_ring(&d3d_device, wgpu_device, render_width, render_height)
        .ok_or_else(|| "Webcam zero-copy decode ring init failed".to_string())?;
    println!(
        "[Export] Zero-copy GPU webcam decode path ({}-slot ring, {}x{})",
        DECODE_RING_SIZE, render_width, render_height
    );

    Ok(Some(WebcamDecodeSetup {
        d3d_device,
        d3d_context,
        ring,
        source_times: source_times
            .iter()
            .map(|time| time - config.webcam_offset_sec)
            .collect(),
        active_mask,
        source_width,
        source_height,
        render_width,
        render_height,
    }))
}

struct ComScope(bool);

impl ComScope {
    fn initialize_mta() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| format!("CoInitializeEx render thread: {e}"))?;
        Ok(Self(true))
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
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
    usage: wgpu::TextureUsages,
) -> Result<wgpu::Texture, String> {
    use windows::Win32::Graphics::Direct3D12 as d3d12;

    let hal_dev = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }
        .ok_or_else(|| "No DX12 HAL device".to_string())?;

    // wgpu-hal's raw_device() returns &windows_058::ID3D12Device.
    // Reinterpret as our windows 0.62 type — same COM vtable, same ABI.
    let hal_d12_ref = hal_dev.raw_device();
    let our_d12: &d3d12::ID3D12Device = unsafe { &*(hal_d12_ref as *const _) };

    // Open the shared NT handle → D3D12 resource (windows 0.62).
    let mut d3d12_resource: Option<d3d12::ID3D12Resource> = None;
    unsafe {
        our_d12
            .OpenSharedHandle(handle, &mut d3d12_resource)
            .map_err(|e| format!("OpenSharedHandle: {e}"))?;
    }
    let d3d12_resource =
        d3d12_resource.ok_or_else(|| "OpenSharedHandle returned null".to_string())?;

    // Convert 0.62 ID3D12Resource → 0.58 for texture_from_raw.
    // Both are pointer-width COM wrappers — bitwise identical.
    let hal_resource = unsafe { std::mem::transmute_copy(&d3d12_resource) };
    std::mem::forget(d3d12_resource); // ownership transferred, prevent double-Release

    let hal_texture = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
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
        )
    };

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
        usage,
        view_formats: &[],
    };

    Ok(unsafe { device.create_texture_from_hal::<wgpu::hal::api::Dx12>(hal_texture, &desc) })
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
    let mut dx12_keyed_mutexes = Vec::with_capacity(GPU_RING_SIZE);

    for i in 0..GPU_RING_SIZE {
        // Use keyed mutex for GPU cache coherence between DX12 (render) and D3D11 (encode).
        let buf = match SharedVramBuffer::new(enc_device, width, height, true) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[Export] SharedVramBuffer[{i}] failed: {e}");
                return None;
            }
        };
        // COPY_DST: render thread writes (copy_output_to_shared).
        // COPY_SRC: state-reset read after each write — forces wgpu to insert a
        //   COPY_SRC → COPY_DST barrier (with cache flush) on the next frame.
        let tex = match unsafe {
            import_shared_handle_into_wgpu(
                wgpu_device,
                buf.handle,
                width,
                height,
                wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            )
        } {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[Export] wgpu import[{i}] failed: {e}");
                return None;
            }
        };
        let km = match buf.texture.cast::<IDXGIKeyedMutex>() {
            Ok(k) => k,
            Err(e) => {
                eprintln!("[Export] Encode keyed mutex[{i}] QI failed: {e}");
                return None;
            }
        };
        shared_buffers.push(buf);
        wgpu_textures.push(tex);
        dx12_keyed_mutexes.push(km);
    }

    Some(GpuOutputRing {
        shared_buffers,
        wgpu_textures,
        dx12_keyed_mutexes,
    })
}

/// Try to create a decode input ring (shared VRAM textures for decode→render).
///
/// Uses D3D11-created `SHARED_KEYEDMUTEX | SHARED_NTHANDLE` textures with cross-API
/// shared fence. wgpu imports with `COPY_SRC | COPY_DST` so the render thread can
/// force a COPY_DST→COPY_SRC barrier (L2 cache flush) each frame via a 1-pixel
/// `copy_buffer_to_texture` before the full `copy_texture_to_texture`.
fn try_create_decode_input_ring(
    dec_device: &ID3D11Device,
    wgpu_device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Option<DecodeInputRing> {
    if std::env::var("SGT_FORCE_CPU_DECODE").is_ok() {
        eprintln!("[Export] SGT_FORCE_CPU_DECODE: forcing CPU decode path");
        return None;
    }

    // ── Create cross-API shared fence ──────────────────────────────────────

    let d3d12_device: d3d12::ID3D12Device = unsafe {
        let Some(hal_dev) = wgpu_device.as_hal::<wgpu::hal::api::Dx12>() else {
            eprintln!("[Export] Failed to get ID3D12Device from wgpu");
            return None;
        };
        let d12_ref = hal_dev.raw_device();
        let d12_ptr: *const d3d12::ID3D12Device = d12_ref as *const _;
        (*d12_ptr).clone()
    };

    let d3d12_fence: d3d12::ID3D12Fence = match unsafe {
        d3d12_device.CreateFence::<d3d12::ID3D12Fence>(0, d3d12::D3D12_FENCE_FLAG_SHARED)
    } {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[Export] ID3D12Device::CreateFence(SHARED) failed: {e}");
            return None;
        }
    };

    let fence_handle =
        match unsafe { d3d12_device.CreateSharedHandle(&d3d12_fence, None, GENERIC_ALL.0, None) } {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[Export] CreateSharedHandle for fence failed: {e}");
                return None;
            }
        };

    let d3d11_device5: ID3D11Device5 = match dec_device.cast() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[Export] Cast to ID3D11Device5 failed: {e}");
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(fence_handle);
            }
            return None;
        }
    };

    let d3d11_fence: ID3D11Fence = {
        let mut f: Option<ID3D11Fence> = None;
        if let Err(e) = unsafe { d3d11_device5.OpenSharedFence(fence_handle, &mut f) } {
            eprintln!("[Export] OpenSharedFence failed: {e}");
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(fence_handle);
            }
            return None;
        }
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(fence_handle);
        }
        f.unwrap()
    };

    eprintln!("[Export] Cross-API shared fence created (D3D12→D3D11)");

    // ── Create shared texture ring ───────────────────────────────────────
    // SHARED_NTHANDLE requires SHARED_KEYEDMUTEX (D3D11 API constraint).
    // Keyed mutex provides CPU-level ownership. The shared fence provides
    // GPU ordering. A per-frame COPY_DST→COPY_SRC barrier forces L2 flush.

    let mut shared_buffers = Vec::with_capacity(DECODE_RING_SIZE);
    let mut wgpu_textures = Vec::with_capacity(DECODE_RING_SIZE);
    let mut keyed_mutexes = Vec::with_capacity(DECODE_RING_SIZE);

    for i in 0..DECODE_RING_SIZE {
        let buf = match SharedVramBuffer::new(dec_device, width, height, true) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[Export] Decode SharedVramBuffer[{i}] failed: {e}");
                return None;
            }
        };
        // COPY_SRC: read source for copy_texture_to_texture.
        // COPY_DST: target for 1-pixel copy_buffer_to_texture that forces a
        //   COPY_DST→COPY_SRC barrier (with L2 cache flush) on the next copy.
        let tex = match unsafe {
            import_shared_handle_into_wgpu(
                wgpu_device,
                buf.handle,
                width,
                height,
                wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            )
        } {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[Export] wgpu decode import[{i}] failed: {e}");
                return None;
            }
        };
        let km = match buf.texture.cast::<IDXGIKeyedMutex>() {
            Ok(k) => k,
            Err(e) => {
                eprintln!("[Export] Decode keyed mutex[{i}] QI failed: {e}");
                return None;
            }
        };
        shared_buffers.push(buf);
        wgpu_textures.push(tex);
        keyed_mutexes.push(km);
    }

    Some(DecodeInputRing {
        shared_buffers,
        wgpu_textures,
        keyed_mutexes,
        d3d11_fence,
        d3d12_fence,
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

    // ─── Device creation (before thread::scope for shared texture init) ─────
    //
    // CRITICAL: Create D3D11 devices on the SAME adapter as wgpu (DX12).
    let t_pipeline_setup = Instant::now();
    // On multi-GPU systems (iGPU + dGPU), D3D11CreateDevice(D3D_DRIVER_TYPE_HARDWARE)
    // picks the default adapter which may be the iGPU, while wgpu picks the dGPU.
    // Shared textures between different adapters don't share VRAM — D3D12 reads
    // stale data because D3D11 wrote to a completely different GPU's memory.

    // Get the wgpu adapter's vendor/device for matching.
    let (wgpu_vendor, wgpu_device_id) = unsafe {
        if let Some(hal_dev) = compositor.device().as_hal::<wgpu::hal::api::Dx12>() {
            let d12_ref = hal_dev.raw_device();
            let d12: &d3d12::ID3D12Device = &*(d12_ref as *const _);
            // GetAdapterLuid gives us the LUID; but we need vendor/device.
            // Query via DXGI instead: QI to IDXGIDevice, get adapter.
            if let Ok(dxgi_dev) = d12.cast::<windows::Win32::Graphics::Dxgi::IDXGIDevice>() {
                if let Ok(adapter) = dxgi_dev.GetAdapter() {
                    if let Ok(desc) = adapter.GetDesc() {
                        (desc.VendorId, desc.DeviceId)
                    } else {
                        (0, 0)
                    }
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    };

    // Decode D3D11 device — MUST be on the same adapter as wgpu for shared texture coherence.
    let (dec_device, dec_context) = if wgpu_vendor != 0 {
        create_d3d11_device_on_adapter(wgpu_vendor, wgpu_device_id)?
    } else {
        create_d3d11_device()?
    };
    {
        let mt: ID3D11Multithread = dec_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (dec): {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }

    // Encode D3D11 device — same adapter requirement.
    let (enc_device, _enc_context) = if wgpu_vendor != 0 {
        create_d3d11_device_on_adapter(wgpu_vendor, wgpu_device_id)?
    } else {
        create_d3d11_device()?
    };
    {
        let mt: ID3D11Multithread = enc_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (enc): {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }
    let enc_device_manager = DxgiDeviceManager::new(&enc_device)?;
    eprintln!(
        "[Pipeline][Timing] D3D11 devices: {:.3}s",
        t_pipeline_setup.elapsed().as_secs_f64()
    );

    let force_cpu = std::env::var("SGT_FORCE_CPU_ENCODE").is_ok();

    // ─── Zero-copy decode input ring (decode → render) ──────────────────────
    let t_rings = Instant::now();

    let decode_ring = if force_cpu {
        None
    } else {
        match try_create_decode_input_ring(
            &dec_device,
            compositor.device(),
            config.video_width,
            config.video_height,
        ) {
            Some(ring) => {
                println!(
                    "[Export] Zero-copy GPU decode path ({}-slot ring)",
                    DECODE_RING_SIZE
                );
                Some(ring)
            }
            None => {
                println!("[Export] Decode ring init failed, falling back to CPU decode");
                None
            }
        }
    };
    let use_gpu_decode = decode_ring.is_some();

    // ─── Zero-copy output ring (render → encode) ────────────────────────────

    let gpu_ring = if force_cpu {
        println!(
            "[Export] Falling back to CPU output path (force_cpu={})",
            force_cpu
        );
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
                    "[Export] Zero-copy GPU output path ({}-slot ring)",
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
    let use_gpu_encode = gpu_ring.is_some();

    // Borrow decode ring contents for the scoped threads.
    let dec_wgpu_textures: &[wgpu::Texture] = decode_ring
        .as_ref()
        .map(|r| r.wgpu_textures.as_slice())
        .unwrap_or(&[]);
    let dec_d3d_textures: Vec<ID3D11Texture2D> = decode_ring
        .as_ref()
        .map(|r| r.shared_buffers.iter().map(|b| b.texture.clone()).collect())
        .unwrap_or_default();
    let dec_keyed_mutexes: &[IDXGIKeyedMutex] = decode_ring
        .as_ref()
        .map(|r| r.keyed_mutexes.as_slice())
        .unwrap_or(&[]);
    // Cross-API shared fence: D3D11 fence for decode thread, D3D12 fence for render thread.
    let dec_d3d11_fence: Option<&ID3D11Fence> = decode_ring.as_ref().map(|r| &r.d3d11_fence);
    let dec_d3d12_fence: Option<&d3d12::ID3D12Fence> = decode_ring.as_ref().map(|r| &r.d3d12_fence);
    let gpu_wgpu_textures: &[wgpu::Texture] = gpu_ring
        .as_ref()
        .map(|r| r.wgpu_textures.as_slice())
        .unwrap_or(&[]);
    let gpu_shared_buffers: &[SharedVramBuffer] = gpu_ring
        .as_ref()
        .map(|r| r.shared_buffers.as_slice())
        .unwrap_or(&[]);
    // DX12-side keyed mutexes for the render thread (encode ring).
    let gpu_dx12_keyed_mutexes: &[IDXGIKeyedMutex] = gpu_ring
        .as_ref()
        .map(|r| r.dx12_keyed_mutexes.as_slice())
        .unwrap_or(&[]);
    let webcam_setup = prepare_webcam_decode_setup(
        source_times,
        config,
        wgpu_vendor,
        wgpu_device_id,
        compositor.device(),
    )?;
    let webcam_enabled = webcam_setup.is_some();
    let webcam_wgpu_textures: &[wgpu::Texture] = webcam_setup
        .as_ref()
        .map(|setup| setup.ring.wgpu_textures.as_slice())
        .unwrap_or(&[]);
    let webcam_d3d_textures: Vec<ID3D11Texture2D> = webcam_setup
        .as_ref()
        .map(|setup| {
            setup
                .ring
                .shared_buffers
                .iter()
                .map(|b| b.texture.clone())
                .collect()
        })
        .unwrap_or_default();
    let webcam_keyed_mutexes: &[IDXGIKeyedMutex] = webcam_setup
        .as_ref()
        .map(|setup| setup.ring.keyed_mutexes.as_slice())
        .unwrap_or(&[]);
    let webcam_d3d11_fence: Option<&ID3D11Fence> =
        webcam_setup.as_ref().map(|setup| &setup.ring.d3d11_fence);
    let webcam_d3d12_fence: Option<&d3d12::ID3D12Fence> =
        webcam_setup.as_ref().map(|setup| &setup.ring.d3d12_fence);
    let webcam_source_times: &[f64] = webcam_setup
        .as_ref()
        .map(|setup| setup.source_times.as_slice())
        .unwrap_or(&[]);
    let webcam_active_mask: Option<&[bool]> = webcam_setup
        .as_ref()
        .map(|setup| setup.active_mask.as_slice());
    let webcam_d3d_device: Option<&ID3D11Device> =
        webcam_setup.as_ref().map(|setup| &setup.d3d_device);
    let webcam_d3d_context: Option<&ID3D11DeviceContext> =
        webcam_setup.as_ref().map(|setup| &setup.d3d_context);
    let webcam_source_width = webcam_setup
        .as_ref()
        .map(|setup| setup.source_width)
        .unwrap_or(0);
    let webcam_source_height = webcam_setup
        .as_ref()
        .map(|setup| setup.source_height)
        .unwrap_or(0);
    let webcam_render_width = webcam_setup
        .as_ref()
        .map(|setup| setup.render_width)
        .unwrap_or(0);
    let webcam_render_height = webcam_setup
        .as_ref()
        .map(|setup| setup.render_height)
        .unwrap_or(0);

    eprintln!(
        "[Pipeline][Timing] Rings + webcam setup: {:.3}s",
        t_rings.elapsed().as_secs_f64()
    );

    // ─── Channels ───────────────────────────────────────────────────────────

    // Forward channels (decode → render → encode).
    let (decode_tx, render_rx) = mpsc::sync_channel::<DecodeOutput>(3);
    let (render_tx, encode_rx) = mpsc::sync_channel::<RenderOutput>(3);
    let (webcam_decode_tx, webcam_render_rx) = if webcam_enabled {
        let (tx, rx) = mpsc::sync_channel::<DecodeOutput>(3);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Decode recycle: GPU path returns ring indices, CPU path returns Vec<u8>.
    let (dec_gpu_recycle_tx, dec_gpu_recycle_rx) = mpsc::channel::<usize>();
    let (dec_cpu_recycle_tx, dec_cpu_recycle_rx) = mpsc::channel::<Vec<u8>>();
    if use_gpu_decode {
        for i in 0..DECODE_RING_SIZE {
            let _ = dec_gpu_recycle_tx.send(i);
        }
    }
    let (webcam_recycle_tx, webcam_recycle_rx) = if webcam_enabled {
        let (tx, rx) = mpsc::channel::<usize>();
        for i in 0..DECODE_RING_SIZE {
            let _ = tx.send(i);
        }
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Encode recycle: GPU path returns ring indices, CPU path returns Vec<u8>.
    let (cpu_recycle_tx, cpu_recycle_rx) = mpsc::channel::<Vec<u8>>();
    let (gpu_recycle_tx, gpu_recycle_rx) = mpsc::channel::<usize>();
    if use_gpu_encode {
        for i in 0..GPU_RING_SIZE {
            let _ = gpu_recycle_tx.send(i);
        }
    }

    let decode_error: std::sync::Arc<Mutex<Option<String>>> = std::sync::Arc::new(Mutex::new(None));
    let webcam_decode_error: std::sync::Arc<Mutex<Option<String>>> =
        std::sync::Arc::new(Mutex::new(None));
    let render_error: std::sync::Arc<Mutex<Option<String>>> = std::sync::Arc::new(Mutex::new(None));

    let mut result: Result<ZeroCopyExportResult, String> = Err("pipeline did not run".into());
    let decode_err_clone = decode_error.clone();
    let webcam_decode_err_clone = webcam_decode_error.clone();
    let render_err_clone = render_error.clone();

    let decode_label = if use_gpu_decode { "GPU" } else { "CPU" };
    let encode_label = if use_gpu_encode { "GPU" } else { "CPU" };
    println!(
        "[Pipeline] {} frames, {}x{} → {}x{} @ {}fps, decode={}, encode={}, blur={} (z={:.2}, p={:.2}, c={:.2}, mb={}), segs={}",
        total_frames,
        config.video_width,
        config.video_height,
        config.output_width,
        config.output_height,
        config.framerate,
        decode_label,
        encode_label,
        config.motion_blur_samples,
        config.blur_zoom_shutter,
        config.blur_pan_shutter,
        config.blur_cursor_shutter,
        mb_enabled,
        config.trim_segments.len()
    );

    eprintln!(
        "[Pipeline][Timing] Total pipeline setup: {:.3}s",
        start.elapsed().as_secs_f64()
    );

    std::thread::scope(|s| {
        // Thread 1: Decode
        s.spawn(move || {
            let result = if use_gpu_decode {
                run_decode_thread(DecodeThreadContext {
                    label: "primary",
                    cancel_flag,
                    source_video_path: &config.source_video_path,
                    source_times,
                    speed_points: &config.speed_points,
                    trim_segments: &config.trim_segments,
                    framerate: config.framerate,
                    crop_x: config.crop_x,
                    crop_y: config.crop_y,
                    source_rect_width: config.video_width,
                    source_rect_height: config.video_height,
                    output_width: config.video_width,
                    output_height: config.video_height,
                    active_mask: None,
                    d3d11_device: &dec_device,
                    d3d11_context: &dec_context,
                    tx: decode_tx,
                    recycle_rx: dec_gpu_recycle_rx,
                    shared_textures: &dec_d3d_textures,
                    keyed_mutexes: dec_keyed_mutexes,
                    d3d11_fence: dec_d3d11_fence,
                })
            } else {
                run_decode_thread_cpu(
                    config,
                    cancel_flag,
                    source_times,
                    decode_tx,
                    dec_cpu_recycle_rx,
                )
            };
            if let Err(e) = result {
                cancel_flag.store(true, Ordering::Relaxed);
                *decode_err_clone.lock().unwrap() = Some(e);
            }
        });
        if let (Some(webcam_tx), Some(webcam_rx)) = (webcam_decode_tx, webcam_recycle_rx) {
            s.spawn(move || {
                let result = run_decode_thread(DecodeThreadContext {
                    label: "webcam",
                    cancel_flag,
                    source_video_path: config
                        .webcam_video_path
                        .as_deref()
                        .expect("webcam path missing while webcam decode is enabled"),
                    source_times: webcam_source_times,
                    speed_points: &config.speed_points,
                    trim_segments: &config.trim_segments,
                    framerate: config.framerate,
                    crop_x: 0,
                    crop_y: 0,
                    source_rect_width: webcam_source_width.max(1),
                    source_rect_height: webcam_source_height.max(1),
                    output_width: webcam_render_width.max(1),
                    output_height: webcam_render_height.max(1),
                    active_mask: webcam_active_mask,
                    d3d11_device: webcam_d3d_device
                        .expect("webcam device missing while webcam decode is enabled"),
                    d3d11_context: webcam_d3d_context
                        .expect("webcam context missing while webcam decode is enabled"),
                    tx: webcam_tx,
                    recycle_rx: webcam_rx,
                    shared_textures: &webcam_d3d_textures,
                    keyed_mutexes: webcam_keyed_mutexes,
                    d3d11_fence: webcam_d3d11_fence,
                });
                if let Err(e) = result {
                    cancel_flag.store(true, Ordering::Relaxed);
                    *webcam_decode_err_clone.lock().unwrap() = Some(e);
                }
            });
        }

        // Thread 2: Render (compositor)
        s.spawn(move || {
            if let Err(e) = run_render_thread(RenderThreadContext {
                config,
                compositor,
                build_uniforms,
                cancel_flag,
                rx: render_rx,
                mb_samples,
                tx: render_tx,
                dec_gpu_recycle_tx,
                dec_cpu_recycle_tx,
                dec_wgpu_textures,
                dec_d3d12_fence,
                dec_keyed_mutexes,
                webcam_rx: webcam_render_rx,
                webcam_gpu_recycle_tx: webcam_recycle_tx,
                webcam_wgpu_textures,
                webcam_d3d12_fence,
                webcam_keyed_mutexes,
                webcam_render_width,
                webcam_render_height,
                gpu_textures: gpu_wgpu_textures,
                gpu_dx12_keyed_mutexes,
                gpu_slot_rx: gpu_recycle_rx,
                cpu_recycle_rx,
            }) {
                cancel_flag.store(true, Ordering::Relaxed);
                *render_err_clone.lock().unwrap() = Some(e);
            }
        });

        // Main thread: Encode
        result = run_encode_thread(EncodeThreadContext {
            config,
            enc_device_manager: &enc_device_manager,
            progress,
            cancel_flag,
            rx: &encode_rx,
            total_frames,
            start: &start,
            gpu_buffers: gpu_shared_buffers,
            gpu_recycle_tx,
            cpu_recycle_tx,
        });

        if result.is_err() {
            cancel_flag.store(true, Ordering::Relaxed);
        }
    });

    // Surface decode/render errors — prefer the earliest root cause over encode errors.
    let decode_err = decode_error.lock().unwrap().take();
    let webcam_decode_err = webcam_decode_error.lock().unwrap().take();
    let render_err = render_error.lock().unwrap().take();

    match (decode_err, webcam_decode_err, render_err, result) {
        (Some(d), Some(w), Some(r), _) => Err(format!(
            "Primary decode thread: {d}\nWebcam decode thread: {w}\nRender thread: {r}"
        )),
        (Some(d), Some(w), None, _) => Err(format!(
            "Primary decode thread: {d}\nWebcam decode thread: {w}"
        )),
        (Some(d), None, Some(r), _) => {
            Err(format!("Primary decode thread: {d}\nRender thread: {r}"))
        }
        (None, Some(w), Some(r), _) => {
            Err(format!("Webcam decode thread: {w}\nRender thread: {r}"))
        }
        (Some(d), None, None, _) => Err(format!("Primary decode thread: {d}")),
        (None, Some(w), None, _) => Err(format!("Webcam decode thread: {w}")),
        (None, None, Some(r), _) => Err(format!("Render thread: {r}")),
        (None, None, None, res) => res,
    }
}

/// GPU decode thread: VP blits NV12 directly into shared VRAM textures (zero-copy).
///
/// Keeps `cur_decoded`/`next_decoded` DecodedFrames alive for the "hold" case.
/// Per output frame we VP Blt the current NV12 source into a shared ring texture,
/// GPU fence, then send the ring index to the render thread. Re-VP-Blt for held
/// frames is cheap (microseconds on GPU) compared to the old CPU readback (~6ms).
struct DecodeThreadContext<'a> {
    label: &'static str,
    cancel_flag: &'a std::sync::atomic::AtomicBool,
    source_video_path: &'a str,
    source_times: &'a [f64],
    speed_points: &'a [SpeedPoint],
    trim_segments: &'a [TrimSegment],
    framerate: u32,
    crop_x: u32,
    crop_y: u32,
    source_rect_width: u32,
    source_rect_height: u32,
    output_width: u32,
    output_height: u32,
    active_mask: Option<&'a [bool]>,
    d3d11_device: &'a ID3D11Device,
    d3d11_context: &'a ID3D11DeviceContext,
    tx: mpsc::SyncSender<DecodeOutput>,
    recycle_rx: mpsc::Receiver<usize>,
    shared_textures: &'a [ID3D11Texture2D],
    keyed_mutexes: &'a [IDXGIKeyedMutex],
    d3d11_fence: Option<&'a ID3D11Fence>,
}

fn run_decode_thread(context: DecodeThreadContext<'_>) -> Result<(), String> {
    let DecodeThreadContext {
        label,
        cancel_flag,
        source_video_path,
        source_times,
        speed_points,
        trim_segments,
        framerate,
        crop_x,
        crop_y,
        source_rect_width,
        source_rect_height,
        output_width,
        output_height,
        active_mask,
        d3d11_device,
        d3d11_context,
        tx,
        recycle_rx,
        shared_textures,
        keyed_mutexes,
        d3d11_fence,
    } = context;
    let t_thread = Instant::now();

    let gpu_fence = D3D11GpuFence::new(d3d11_device, d3d11_context)?;

    // Cast to ID3D11DeviceContext4 for cross-API fence signaling.
    let d3d11_context4: Option<ID3D11DeviceContext4> =
        d3d11_fence.and_then(|_| d3d11_context.cast::<ID3D11DeviceContext4>().ok());

    let mut fence_value: u64 = 0;

    let t_dec_init = Instant::now();
    let device_manager = DxgiDeviceManager::new(d3d11_device)?;
    let decoder = MfDecoder::new(source_video_path, &device_manager, true)?;
    let source_w = decoder.width();
    let source_h = decoder.height();

    let initial_seek = if !trim_segments.is_empty() {
        trim_segments[0].start_time
    } else {
        source_times.first().copied().unwrap_or(0.0).max(0.0)
    };
    if initial_seek > 0.0 {
        decoder.seek_seconds(initial_seek)?;
    }
    let vp_out_w = output_width;
    let vp_out_h = output_height;
    let decode_vp = VideoProcessor::new(
        d3d11_device,
        d3d11_context,
        source_w,
        source_h,
        vp_out_w,
        vp_out_h,
    )?;
    if crop_x != 0 || crop_y != 0 || source_rect_width != source_w || source_rect_height != source_h
    {
        decode_vp.set_source_rect(crop_x, crop_y, source_rect_width, source_rect_height);
    }

    // Pool of VP output textures for B-frame PTS reorder.
    // Hardware decoders deliver B-frames in decode order (non-monotonic PTS).
    // We fill a window of REORDER_WINDOW frames, sort by PTS, then output in
    // display order — fixing the "back and forth frames" issue on B-frame content.
    const REORDER_WINDOW: usize = 6;
    let pool_size = REORDER_WINDOW + 2; // +2 for cur + next held simultaneously
    let mut vp_pool: Vec<ID3D11Texture2D> = Vec::with_capacity(pool_size);
    let mut vp_resources: Vec<ID3D11Resource> = Vec::with_capacity(pool_size);
    let mut free_vp_slots: Vec<usize> = (0..pool_size).rev().collect();

    for _ in 0..pool_size {
        let tex = VideoProcessor::create_texture(
            d3d11_device,
            vp_out_w,
            vp_out_h,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
        )?;
        vp_resources.push(tex.cast().map_err(|e| format!("vp_pool→Resource: {e}"))?);
        vp_pool.push(tex);
    }

    // Pre-cast shared ring textures to ID3D11Resource (avoids per-frame cast).
    let shared_resources: Vec<ID3D11Resource> = shared_textures
        .iter()
        .map(|t| t.cast().map_err(|e| format!("shared_ring→Resource: {e}")))
        .collect::<Result<_, _>>()?;

    // Reorder queue: (vp_slot, pts). Sorted descending so pop() yields the lowest PTS.
    let mut reorder_queue: Vec<(usize, f64)> = Vec::with_capacity(REORDER_WINDOW);
    let mut eof_reached = false;
    let mut src_decoded: u32 = 0;

    // Holds DecodedFrames (IMFSamples) alive while VP Blts are pending on the GPU.
    // VP Blt reads from the decoder's NV12 texture asynchronously; if the IMFSample is
    // dropped too early, MF recycles the texture subresource and NVDEC (a separate HW
    // engine) can overwrite it before the VP Blt finishes reading — causing frame corruption.
    // We collect frames here, then gpu_fence.signal_and_wait() after all VP Blts are queued,
    // ensuring they complete before the decoder can recycle any texture.
    let mut pending_samples: Vec<DecodedFrame> = Vec::new();

    // Diagnostic: log raw decoder PTS and output frame selection for first N frames.
    let decode_debug = false;

    // Fill the reorder queue from the decoder up to REORDER_WINDOW entries.
    // VP-converts each frame at read time (decode order), then sorts by PTS (display order).
    macro_rules! fill_reorder_queue {
        () => {
            if !eof_reached {
                while reorder_queue.len() < REORDER_WINDOW {
                    let slot = match free_vp_slots.pop() {
                        Some(s) => s,
                        None => break, // all slots busy — queue is as full as possible
                    };
                    match decoder.read_frame()? {
                        Some(f) => {
                            decode_vp.convert(&f.texture, f.subresource_index, &vp_pool[slot])?;
                            let pts = f.pts_100ns as f64 / 10_000_000.0;
                            if decode_debug && src_decoded < 40 {
                                eprintln!(
                                    "[DecDbg] raw#{} pts={:.4} slot={}",
                                    src_decoded, pts, slot
                                );
                            }
                            reorder_queue.push((slot, pts));
                            pending_samples.push(f); // keep IMFSample alive until VP Blt done
                            src_decoded += 1;
                        }
                        None => {
                            eof_reached = true;
                            free_vp_slots.push(slot);
                            break;
                        }
                    }
                }
                // Ensure all VP Blts complete before releasing decoder textures.
                // NVDEC runs on a separate HW engine from the VP — without this fence,
                // the decoder can overwrite source NV12 textures while VP is still reading.
                if !pending_samples.is_empty() {
                    gpu_fence.signal_and_wait();
                    pending_samples.clear();
                }
                // Sort descending; pop() then gives the frame with the lowest PTS.
                reorder_queue
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        };
    }

    eprintln!(
        "[Decode:{label}] Init (MfDecoder + VP + pool): {:.3}s",
        t_dec_init.elapsed().as_secs_f64()
    );

    fill_reorder_queue!();
    let fa = reorder_queue.pop();
    fill_reorder_queue!();
    let fb = reorder_queue.pop();

    let mut cur_slot: Option<usize> = fa.map(|(s, _)| s);
    let mut _cur_pts: f64 = fa.map(|(_, p)| p).unwrap_or(0.0);
    let mut next_slot: Option<usize> = fb.map(|(s, _)| s);
    let mut next_pts: f64 = fb.map(|(_, p)| p).unwrap_or(f64::MAX);
    let mut have_next = next_slot.is_some();

    // Flush all reorder state, seek, and re-prime with fresh frames.
    // Must be defined AFTER the `let mut cur_slot/next_slot/cur_pts/next_pts/have_next`
    // declarations — macro_rules! resolves bare identifiers at the definition site.
    macro_rules! flush_and_refill {
        () => {
            if let Some(s) = cur_slot.take() {
                free_vp_slots.push(s);
            }
            if let Some(s) = next_slot.take() {
                free_vp_slots.push(s);
            }
            for (s, _) in reorder_queue.drain(..) {
                free_vp_slots.push(s);
            }
            eof_reached = false;
            fill_reorder_queue!();
            let fa = reorder_queue.pop();
            fill_reorder_queue!();
            let fb = reorder_queue.pop();
            cur_slot = fa.map(|(s, _)| s);
            _cur_pts = fa.map(|(_, p)| p).unwrap_or(0.0);
            next_slot = fb.map(|(s, _)| s);
            next_pts = fb.map(|(_, p)| p).unwrap_or(f64::MAX);
            have_next = next_slot.is_some();
        };
    }

    if cur_slot.is_none() {
        return Ok(());
    }

    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;
    let mut slow_frame_count: u32 = 0;
    for (frame_idx, &source_time) in source_times.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
        let frame_t0 = Instant::now();

        let next_source_time = source_times
            .get(frame_idx + 1)
            .copied()
            .unwrap_or(source_time);
        let speed = get_speed(source_time, speed_points).clamp(0.1, 16.0);
        let expected_step = speed / framerate.max(1) as f64;
        let mut source_step = next_source_time - source_time;
        if source_step <= 0.0 || source_step > expected_step * 1.05 {
            source_step = expected_step;
        }

        if active_mask.is_some_and(|mask| !mask[frame_idx]) {
            if tx
                .send(DecodeOutput::Inactive {
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
            continue;
        }

        let mut source_changed = false;

        // Seek on trim segment boundary change.
        if !trim_segments.is_empty() {
            let target_seg = trim_segments
                .iter()
                .position(|s| {
                    source_time >= s.start_time - 0.001 && source_time <= s.end_time + 0.001
                })
                .unwrap_or(current_segment_idx);

            if target_seg != current_segment_idx {
                decoder.seek_seconds(trim_segments[target_seg].start_time)?;
                current_segment_idx = target_seg;
                flush_and_refill!();
                source_changed = true;
            }
        }

        // Fast-forward seek if source_time is >1.5s ahead.
        if have_next && source_time - next_pts > 1.5 {
            decoder.seek_seconds(source_time)?;
            flush_and_refill!();
            source_changed = true;
        }

        let mut advanced = false;
        while have_next && next_pts <= source_time {
            if let Some(s) = cur_slot.take() {
                free_vp_slots.push(s);
            }
            cur_slot = next_slot.take();
            _cur_pts = next_pts;
            advanced = true;

            fill_reorder_queue!();
            if let Some((s, p)) = reorder_queue.pop() {
                next_slot = Some(s);
                next_pts = p;
            } else {
                have_next = false;
                next_slot = None;
                next_pts = f64::MAX;
            }
        }

        if !advanced && !source_changed && frame_idx > 0 {
            frames_held += 1;
            if tx
                .send(DecodeOutput::GpuHold {
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
            continue;
        }

        // Acquire a free ring slot (blocks if all slots are in use by render thread).
        let ring_idx = match recycle_rx.try_recv() {
            Ok(idx) => idx,
            Err(mpsc::TryRecvError::Empty) => match recycle_rx.recv() {
                Ok(idx) => idx,
                Err(_) => break,
            },
            Err(_) => break,
        };

        let cur_vp_slot = match cur_slot {
            Some(s) => s,
            None => break,
        };

        if decode_debug && frame_idx < 40 {
            eprintln!(
                "[DecDbg] OUT f{} src_t={:.4} cur_pts={:.4} next_pts={:.4} slot={} adv={}",
                frame_idx, source_time, _cur_pts, next_pts, cur_vp_slot, advanced
            );
        }

        // Acquire keyed mutex for the shared decode ring slot (CPU-side ownership).
        if !keyed_mutexes.is_empty() {
            unsafe {
                keyed_mutexes[ring_idx]
                    .AcquireSync(0, u32::MAX)
                    .map_err(|e| format!("AcquireSync dec[{ring_idx}]: {e}"))?;
            }
        }

        // Copy VP output to shared decode ring slot.
        unsafe {
            d3d11_context.CopyResource(&shared_resources[ring_idx], &vp_resources[cur_vp_slot]);
        }

        // Signal cross-API fence AFTER the CopyResource.
        // This queues a GPU-timeline signal on D3D11's command queue. The render thread
        // calls ID3D12CommandQueue::Wait on this fence value, which stalls DX12's queue
        // until D3D11's GPU work completes — providing both ordering AND cache coherence.
        fence_value += 1;
        if let (Some(ctx4), Some(fence)) = (&d3d11_context4, d3d11_fence) {
            unsafe {
                ctx4.Signal(fence, fence_value)
                    .map_err(|e| format!("D3D11 Signal fence[{ring_idx}]: {e}"))?;
            }
        }

        // GPU fence: ensure CopyResource + Signal are committed to GPU queue.
        gpu_fence.signal_and_wait();

        // Release keyed mutex — D3D11 is done writing, render thread can acquire.
        if !keyed_mutexes.is_empty() {
            unsafe {
                let _ = keyed_mutexes[ring_idx].ReleaseSync(0);
            }
        }

        let frame_dur = frame_t0.elapsed().as_secs_f64();
        if frame_dur > 0.5 {
            slow_frame_count += 1;
            eprintln!(
                "[Decode:{label}] SLOW f{frame_idx}/{} src_t={source_time:.3} took {frame_dur:.3}s",
                source_times.len()
            );
        }

        if tx
            .send(DecodeOutput::Gpu {
                ring_idx,
                source_time,
                source_step,
                frame_idx: frame_idx as u32,
                fence_value,
            })
            .is_err()
        {
            break;
        }
    }

    if slow_frame_count > 0 {
        eprintln!("[Decode:{label}] {slow_frame_count} slow frames (>0.5s each)");
    }
    let elapsed = t_thread.elapsed().as_secs_f64();
    let fps = if elapsed > 0.001 {
        source_times.len() as f64 / elapsed
    } else {
        0.0
    };
    println!(
        "[Decode:{}] GPU: {} src → {} out ({} held) in {:.1}s ({:.1} out_fps)",
        label,
        src_decoded,
        source_times.len(),
        frames_held,
        elapsed,
        fps
    );
    Ok(())
}

/// CPU fallback decode thread: D3D11 VP + CPU readback (legacy path).
///
/// `cur_bgra` and `next_bgra` are PERMANENT buffers owned by this thread (never sent across
/// threads). Per output frame we copy `cur_bgra` into a recycled `send_buf` and send that.
/// This correctly handles the "hold" case (same source frame reused for multiple output frames)
/// which occurs whenever output fps > source fps.
fn run_decode_thread_cpu(
    config: &PipelineConfig,
    cancel_flag: &std::sync::atomic::AtomicBool,
    source_times: &[f64],
    tx: mpsc::SyncSender<DecodeOutput>,
    recycle_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), String> {
    let t_thread = Instant::now();

    // D3D11 device (CPU decode thread creates its own)
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

    // B-frame reorder buffer: same window approach as the GPU path.
    // decode_one (VP + readback) already runs at fill time; queue holds decoded BGRA.
    const REORDER_WINDOW_CPU: usize = 6;
    let mut free_bufs: Vec<Vec<u8>> = Vec::new();
    let mut reorder_queue_cpu: Vec<(Vec<u8>, f64)> = Vec::with_capacity(REORDER_WINDOW_CPU);
    let mut eof_reached_cpu = false;
    let mut src_decoded: u32 = 0;

    macro_rules! get_buf {
        () => {
            free_bufs.pop().unwrap_or_else(|| {
                let mut b = Vec::with_capacity(frame_size);
                b.resize(frame_size, 0);
                b
            })
        };
    }

    macro_rules! fill_reorder_cpu {
        () => {
            if !eof_reached_cpu {
                while reorder_queue_cpu.len() < REORDER_WINDOW_CPU {
                    let mut buf = get_buf!();
                    match decode_one(&mut buf)? {
                        Some(pts) => {
                            reorder_queue_cpu.push((buf, pts));
                            src_decoded += 1;
                        }
                        None => {
                            eof_reached_cpu = true;
                            free_bufs.push(buf);
                            break;
                        }
                    }
                }
                reorder_queue_cpu
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        };
    }

    fill_reorder_cpu!();
    let fa = reorder_queue_cpu.pop();
    fill_reorder_cpu!();
    let fb = reorder_queue_cpu.pop();

    let (mut cur_bgra, mut cur_pts): (Option<Vec<u8>>, f64) =
        fa.map(|(b, p)| (Some(b), p)).unwrap_or((None, 0.0));
    let (mut next_bgra, mut next_pts): (Option<Vec<u8>>, f64) =
        fb.map(|(b, p)| (Some(b), p)).unwrap_or((None, f64::MAX));
    let mut have_next = next_bgra.is_some();

    // Must be defined AFTER the variable declarations above — see comment on flush_and_refill!.
    macro_rules! flush_and_refill_cpu {
        () => {
            if let Some(b) = cur_bgra.take() {
                free_bufs.push(b);
            }
            if let Some(b) = next_bgra.take() {
                free_bufs.push(b);
            }
            for (b, _) in reorder_queue_cpu.drain(..) {
                free_bufs.push(b);
            }
            eof_reached_cpu = false;
            fill_reorder_cpu!();
            let fa = reorder_queue_cpu.pop();
            fill_reorder_cpu!();
            let fb = reorder_queue_cpu.pop();
            (cur_bgra, cur_pts) = fa.map(|(b, p)| (Some(b), p)).unwrap_or((None, 0.0));
            (next_bgra, next_pts) = fb.map(|(b, p)| (Some(b), p)).unwrap_or((None, f64::MAX));
            have_next = next_bgra.is_some();
        };
    }

    if cur_bgra.is_none() {
        return Ok(());
    }

    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;

    for (frame_idx, &source_time) in source_times.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let next_source_time = source_times
            .get(frame_idx + 1)
            .copied()
            .unwrap_or(source_time);
        let speed = get_speed(source_time, &config.speed_points).clamp(0.1, 16.0);
        let expected_step = speed / config.framerate as f64;
        let mut source_step = next_source_time - source_time;
        if source_step <= 0.0 || source_step > expected_step * 1.05 {
            source_step = expected_step;
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
                flush_and_refill_cpu!();
            }
        }

        // Fast-forward seek if source_time is >1.5s ahead.
        if have_next && source_time - next_pts > 1.5 {
            decoder.seek_seconds(source_time)?;
            flush_and_refill_cpu!();
        }

        let mut advanced = false;
        while have_next && next_pts <= source_time {
            if let Some(b) = cur_bgra.take() {
                free_bufs.push(b);
            }
            cur_bgra = next_bgra.take();
            cur_pts = next_pts;
            advanced = true;

            fill_reorder_cpu!();
            if let Some((b, p)) = reorder_queue_cpu.pop() {
                next_bgra = Some(b);
                next_pts = p;
            } else {
                have_next = false;
                next_bgra = None;
                next_pts = f64::MAX;
            }
        }

        if !advanced && frame_idx > 0 {
            frames_held += 1;
        }
        let _ = cur_pts;

        if let Some(ref bgra) = cur_bgra {
            let mut send_buf = recycle_rx.try_recv().unwrap_or_else(|_| get_buf!());
            send_buf.resize(frame_size, 0);
            send_buf.copy_from_slice(bgra);

            if tx
                .send(DecodeOutput::Cpu {
                    bgra_video: send_buf,
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
        } else {
            break;
        }
    }

    let elapsed = t_thread.elapsed().as_secs_f64();
    println!(
        "[Decode] CPU: {} src → {} out ({} held) in {:.1}s",
        src_decoded,
        source_times.len(),
        frames_held,
        elapsed
    );
    Ok(())
}

/// Render thread: receives decoded frames (GPU ring or CPU buffer), runs compositor,
/// sends output to encoder.
///
/// Decode input: GPU path copies shared decode texture to video_texture (fast GPU copy).
///               CPU path uploads BGRA via queue.write_texture (PCIe upload).
/// Encode output: GPU path copies output to shared VRAM texture, send ring_idx.
///                CPU path: pipelined readback depth 2, send BGRA Vec.
struct RenderThreadContext<'a> {
    config: &'a PipelineConfig,
    compositor: &'a mut GpuCompositor,
    build_uniforms: &'a (dyn Fn(f64, f64, f64, f64) -> CompositorUniforms + Sync),
    cancel_flag: &'a std::sync::atomic::AtomicBool,
    rx: mpsc::Receiver<DecodeOutput>,
    mb_samples: u32,
    tx: mpsc::SyncSender<RenderOutput>,
    // Decode recycle channels (render → decode).
    dec_gpu_recycle_tx: mpsc::Sender<usize>,
    dec_cpu_recycle_tx: mpsc::Sender<Vec<u8>>,
    dec_wgpu_textures: &'a [wgpu::Texture],
    // Cross-API shared fence: DX12 waits for D3D11's Signal before reading shared textures.
    dec_d3d12_fence: Option<&'a d3d12::ID3D12Fence>,
    // Keyed mutexes for decode ring slots (CPU ownership protocol).
    dec_keyed_mutexes: &'a [IDXGIKeyedMutex],
    webcam_rx: Option<mpsc::Receiver<DecodeOutput>>,
    webcam_gpu_recycle_tx: Option<mpsc::Sender<usize>>,
    webcam_wgpu_textures: &'a [wgpu::Texture],
    webcam_d3d12_fence: Option<&'a d3d12::ID3D12Fence>,
    webcam_keyed_mutexes: &'a [IDXGIKeyedMutex],
    webcam_render_width: u32,
    webcam_render_height: u32,
    // Encode output resources.
    gpu_textures: &'a [wgpu::Texture],
    // D3D12-side keyed mutex per encode ring slot.
    // AcquireSync before writing; ReleaseSync after poll(Wait)+on_submitted_work_done.
    gpu_dx12_keyed_mutexes: &'a [IDXGIKeyedMutex],
    gpu_slot_rx: mpsc::Receiver<usize>,
    cpu_recycle_rx: mpsc::Receiver<Vec<u8>>,
}

fn run_render_thread(context: RenderThreadContext<'_>) -> Result<(), String> {
    let _com_scope = ComScope::initialize_mta()?;
    let RenderThreadContext {
        config,
        compositor,
        build_uniforms,
        cancel_flag,
        rx,
        mb_samples,
        tx,
        dec_gpu_recycle_tx,
        dec_cpu_recycle_tx,
        dec_wgpu_textures,
        dec_d3d12_fence,
        dec_keyed_mutexes,
        webcam_rx,
        webcam_gpu_recycle_tx,
        webcam_wgpu_textures,
        webcam_d3d12_fence,
        webcam_keyed_mutexes,
        webcam_render_width,
        webcam_render_height,
        gpu_textures,
        gpu_dx12_keyed_mutexes,
        gpu_slot_rx,
        cpu_recycle_rx,
    } = context;
    let use_gpu_encode = !gpu_textures.is_empty();
    let mut frames_rendered: u32 = 0;
    let mut t_upload = 0.0_f64;
    let mut t_render = 0.0_f64;
    let mut t_readback = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // Pipelined readback state (CPU encode path only).
    let mut queued_readbacks: u32 = 0;

    // GPU decode ring recycle queue.
    // copy_frame_from_shared is async — the ring slot cannot be returned to the decode
    // thread until after poll(Wait) confirms the DX12 read has completed.
    // Each entry is Some(ring_idx) for GPU-decode frames, None for CPU-decode frames.
    // GPU encode: one entry per frame, drained immediately after poll(Wait).
    // CPU encode: entries accumulate with pipelined readbacks and are drained together.
    let mut dec_ring_recycle_queue: std::collections::VecDeque<Option<usize>> =
        std::collections::VecDeque::new();
    let mut webcam_ring_recycle_queue: std::collections::VecDeque<Option<usize>> =
        std::collections::VecDeque::new();

    let mut render_slow_count: u32 = 0;
    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        let wait_dur = tw0.elapsed().as_secs_f64();
        t_wait += wait_dur;

        if cancel_flag.load(Ordering::Relaxed) {
            match &msg {
                DecodeOutput::Gpu { ring_idx, .. } => {
                    let _ = dec_gpu_recycle_tx.send(*ring_idx);
                }
                DecodeOutput::GpuHold { .. } => {}
                DecodeOutput::Cpu { .. } => {}
                DecodeOutput::Inactive { .. } => {}
            }
            break;
        }

        let render_frame_t0 = Instant::now();
        let source_time = msg.source_time();
        let source_step = msg.source_step();
        let frame_idx = msg.frame_idx();

        // 1. Upload video frame to GPU (GPU copy or CPU upload depending on decode path).
        //
        // GPU path: Wait on cross-API shared fence to ensure D3D11's writes are visible
        // on DX12's GPU timeline, then copy from persistent shared texture.
        // Ring slot recycled after poll(Wait) confirms DX12 copy+render complete.
        let tu0 = Instant::now();
        match msg {
            DecodeOutput::Gpu {
                ring_idx,
                fence_value,
                ..
            } => {
                // Acquire keyed mutex on DX12 side (CPU ownership for shared texture).
                if !dec_keyed_mutexes.is_empty() {
                    unsafe {
                        dec_keyed_mutexes[ring_idx]
                            .AcquireSync(0, u32::MAX)
                            .map_err(|e| format!("AcquireSync dec render[{ring_idx}]: {e}"))?;
                    }
                }

                // Wait on DX12 command queue for D3D11's fence Signal.
                // This stalls DX12's GPU pipeline until D3D11 completes writing to the
                // shared texture. The Wait acts as an implicit acquire barrier, invalidating
                // DX12's L2 cache so subsequent reads get fresh VRAM data.
                if let Some(fence) = dec_d3d12_fence {
                    unsafe {
                        if let Some(hal_dev) = compositor.device().as_hal::<wgpu::hal::api::Dx12>()
                        {
                            let raw_queue: &d3d12::ID3D12CommandQueue =
                                &*(hal_dev.raw_queue() as *const _);
                            let _ = raw_queue.Wait(fence, fence_value);
                        }
                    }
                }

                compositor.copy_frame_from_shared(&dec_wgpu_textures[ring_idx]);

                dec_ring_recycle_queue.push_back(Some(ring_idx)); // recycled after poll(Wait)
            }
            DecodeOutput::GpuHold { .. } => {
                // Reuse the previously uploaded frame when the source PTS is held.
                // This avoids unnecessary shared-texture reads on hold frames.
                dec_ring_recycle_queue.push_back(None);
            }
            DecodeOutput::Cpu { bgra_video, .. } => {
                // CPU fallback: PCIe upload (no deferred recycle needed).
                compositor.upload_frame(&bgra_video);
                let _ = dec_cpu_recycle_tx.send(bgra_video);
                dec_ring_recycle_queue.push_back(None);
            }
            DecodeOutput::Inactive { .. } => {
                dec_ring_recycle_queue.push_back(None);
            }
        }
        t_upload += tu0.elapsed().as_secs_f64();

        if let Some(webcam_rx) = webcam_rx.as_ref() {
            let webcam_msg = webcam_rx
                .recv()
                .map_err(|_| "Webcam decode channel closed".to_string())?;
            if webcam_msg.frame_idx() != frame_idx {
                return Err(format!(
                    "Webcam frame index mismatch: expected {}, got {}",
                    frame_idx,
                    webcam_msg.frame_idx()
                ));
            }

            match webcam_msg {
                DecodeOutput::Gpu {
                    ring_idx,
                    fence_value,
                    ..
                } => {
                    if !webcam_keyed_mutexes.is_empty() {
                        unsafe {
                            webcam_keyed_mutexes[ring_idx]
                                .AcquireSync(0, u32::MAX)
                                .map_err(|e| {
                                    format!("AcquireSync webcam render[{ring_idx}]: {e}")
                                })?;
                        }
                    }
                    if let Some(fence) = webcam_d3d12_fence {
                        unsafe {
                            if let Some(hal_dev) =
                                compositor.device().as_hal::<wgpu::hal::api::Dx12>()
                            {
                                let raw_queue: &d3d12::ID3D12CommandQueue =
                                    &*(hal_dev.raw_queue() as *const _);
                                let _ = raw_queue.Wait(fence, fence_value);
                            }
                        }
                    }
                    compositor.copy_webcam_frame_from_shared(
                        &webcam_wgpu_textures[ring_idx],
                        webcam_render_width,
                        webcam_render_height,
                    );
                    webcam_ring_recycle_queue.push_back(Some(ring_idx));
                }
                DecodeOutput::GpuHold { .. } | DecodeOutput::Inactive { .. } => {
                    webcam_ring_recycle_queue.push_back(None);
                }
                DecodeOutput::Cpu { .. } => {
                    return Err("Unexpected CPU webcam decode output".to_string());
                }
            }
        }

        // 1b. Update animated cursor atlas tiles based on current source time.
        for slot in &config.animated_cursor_slots {
            if slot.frames.is_empty() || slot.loop_duration <= 0.0 {
                continue;
            }
            let n = slot.frames.len();
            let t = source_time % slot.loop_duration;
            let idx = ((t / slot.loop_duration) * n as f64).floor() as usize % n;
            compositor.upload_cursor_slot_rgba(slot.slot_id, &slot.frames[idx]);
        }

        // 2. Render to output texture.
        let tr0 = Instant::now();
        if mb_samples > 1 {
            let zoom_shutter = source_step * config.blur_zoom_shutter;
            let pan_shutter = source_step * config.blur_pan_shutter;
            let cursor_shutter = source_step * config.blur_cursor_shutter;

            // Collect all N (uniforms, weight) pairs then dispatch as a single
            // RenderPass — 1 encoder, 1 submit, N draw calls with dynamic offsets.
            let mut passes = Vec::with_capacity(mb_samples as usize);
            for i in 0..mb_samples {
                let t = i as f64 / (mb_samples - 1).max(1) as f64;
                let pan_time = source_time - (pan_shutter * 0.5) + t * pan_shutter;
                let zoom_time = source_time - (zoom_shutter * 0.5) + t * zoom_shutter;
                let cur_time = source_time - (cursor_shutter * 0.5) + t * cursor_shutter;
                let uniforms = build_uniforms(source_time, pan_time, zoom_time, cur_time);
                passes.push((uniforms, 1.0 / (i as f64 + 1.0)));
            }
            compositor.render_accumulate_batched(&passes, None);
        } else {
            let uniforms = build_uniforms(source_time, source_time, source_time, source_time);
            compositor.render_to_output(&uniforms, true, None);
        }

        let webcam_frame = if webcam_rx.is_some() && !config.webcam_frames.is_empty() {
            let webcam_idx = (frame_idx as usize).min(config.webcam_frames.len() - 1);
            Some(&config.webcam_frames[webcam_idx])
        } else {
            None
        };
        let webcam_should_render = webcam_frame.is_some_and(|frame| {
            frame.visible && frame.opacity > 0.001 && frame.width > 0.0 && frame.height > 0.0
        });
        let overlay_quads = if !config.overlay_frames.is_empty() {
            let overlay_idx = (frame_idx as usize).min(config.overlay_frames.len() - 1);
            config.overlay_frames[overlay_idx].quads.as_slice()
        } else {
            &[]
        };

        if webcam_should_render || !overlay_quads.is_empty() {
            compositor.render_post_overlays(
                if webcam_should_render {
                    webcam_frame
                } else {
                    None
                },
                overlay_quads,
            );
        }

        t_render += tr0.elapsed().as_secs_f64();

        // 3. Output: GPU zero-copy or CPU readback.
        if use_gpu_encode {
            // GPU path: copy to shared VRAM texture, wait for GPU, send ring_idx.
            let ring_idx = gpu_slot_rx
                .recv()
                .map_err(|_| "GPU slot recycle channel closed")?;
            let trb0 = Instant::now();

            // Acquire DX12-side keyed mutex before writing to the encode ring slot.
            // This cache-invalidates any stale D3D11 data and satisfies the DXGI
            // keyed-mutex contract for cross-API (DX12↔D3D11) sharing.
            if !gpu_dx12_keyed_mutexes.is_empty() {
                unsafe {
                    gpu_dx12_keyed_mutexes[ring_idx]
                        .AcquireSync(0, u32::MAX)
                        .map_err(|e| format!("AcquireSync DX12 enc[{ring_idx}]: {e}"))?;
                }
            }

            compositor.copy_output_to_shared(&gpu_textures[ring_idx]);

            // Wait for all DX12 work to complete using on_submitted_work_done + poll.
            // This is strictly safer than poll(Wait) alone.
            let (tx_done, rx_done) = std::sync::mpsc::channel();
            compositor.queue().on_submitted_work_done(move || {
                let _ = tx_done.send(());
            });
            let _ = compositor
                .device()
                .poll(wgpu::PollType::wait_indefinitely());
            let _ = rx_done.recv();

            // Release DX12-side keyed mutex for this encode ring slot — flushes DX12
            // caches so D3D11 (MF encoder) sees the freshly written frame data.
            if !gpu_dx12_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = gpu_dx12_keyed_mutexes[ring_idx].ReleaseSync(0);
                }
            }

            // Recycle decode ring slot — GPU work is done (poll+on_submitted_work_done).
            if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
                // Release decode keyed mutex so decode thread can re-acquire.
                if !dec_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                    }
                }
                let _ = dec_gpu_recycle_tx.send(idx);
            }
            if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
                if !webcam_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                    }
                }
                if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                    let _ = tx.send(idx);
                }
            }
            t_readback += trb0.elapsed().as_secs_f64();
            if tx.send(RenderOutput::Gpu { ring_idx }).is_err() {
                break;
            }
            let render_frame_dur = render_frame_t0.elapsed().as_secs_f64();
            if render_frame_dur > 0.5 || wait_dur > 0.5 {
                render_slow_count += 1;
                eprintln!(
                    "[Render] SLOW f{frame_idx} src_t={source_time:.3} \
                     wait={wait_dur:.3}s render={render_frame_dur:.3}s"
                );
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
                // readback_output internally does poll(Wait) — DX12 copy is done.
                if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
                    if !dec_keyed_mutexes.is_empty() {
                        unsafe {
                            let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                        }
                    }
                    let _ = dec_gpu_recycle_tx.send(idx);
                }
                if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
                    if !webcam_keyed_mutexes.is_empty() {
                        unsafe {
                            let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                        }
                    }
                    if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                        let _ = tx.send(idx);
                    }
                }
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

    // CPU encode path: drain remaining GPU readbacks at end of video.
    while queued_readbacks > 0 {
        let trb0 = Instant::now();
        let mut out_buf = cpu_recycle_rx.try_recv().unwrap_or_default();
        compositor.readback_output(&mut out_buf)?;
        if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
            if !dec_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                }
            }
            let _ = dec_gpu_recycle_tx.send(idx);
        }
        if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
            if !webcam_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                }
            }
            if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                let _ = tx.send(idx);
            }
        }
        t_readback += trb0.elapsed().as_secs_f64();
        queued_readbacks -= 1;
        let _ = tx.send(RenderOutput::Cpu {
            rendered_bgra: out_buf,
        });
        frames_rendered += 1;
    }

    if render_slow_count > 0 {
        eprintln!("[Render] {render_slow_count} slow frames (>0.5s each)");
    }
    let n = frames_rendered.max(1) as f64;
    let label = if use_gpu_encode { "copy" } else { "readback" };
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
struct EncodeThreadContext<'a> {
    config: &'a PipelineConfig,
    enc_device_manager: &'a DxgiDeviceManager,
    progress: Option<ProgressCallback>,
    cancel_flag: &'a std::sync::atomic::AtomicBool,
    rx: &'a mpsc::Receiver<RenderOutput>,
    total_frames: u32,
    start: &'a Instant,
    gpu_buffers: &'a [SharedVramBuffer],
    gpu_recycle_tx: mpsc::Sender<usize>,
    cpu_recycle_tx: mpsc::Sender<Vec<u8>>,
}

fn run_encode_thread(context: EncodeThreadContext<'_>) -> Result<ZeroCopyExportResult, String> {
    let EncodeThreadContext {
        config,
        enc_device_manager,
        progress,
        cancel_flag,
        rx,
        total_frames,
        start,
        gpu_buffers,
        gpu_recycle_tx,
        cpu_recycle_tx,
    } = context;
    let t_enc_init = Instant::now();
    let mut audio_decoder = None;
    let mut audio_config = None;

    if let Some(path) = &config.audio_path
        && !path.is_empty()
    {
        match MfAudioDecoder::new(path) {
            Ok(dec) => {
                audio_config = Some(AudioConfig {
                    sample_rate: dec.sample_rate(),
                    channels: dec.channels(),
                    bitrate_kbps: 192,
                });
                audio_decoder = Some(dec);
            }
            Err(e) => eprintln!("[Audio] Failed to open native audio decoder: {e}"),
        }
    }

    let encoder_config = EncoderConfig {
        codec: config.codec,
        width: config.output_width,
        height: config.output_height,
        fps_num: config.framerate,
        fps_den: 1,
        bitrate_kbps: config.bitrate_kbps,
    };
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

    if let Some(dec) = &audio_decoder
        && !config.audio_is_preprocessed
    {
        let start_time = if config.trim_segments.is_empty() {
            config.trim_start
        } else {
            config.trim_segments[0].start_time
        };
        if start_time > 0.0 {
            let _ = dec.seek((start_time * 10_000_000.0) as i64);
        }
    }

    eprintln!(
        "[Encode] Init (audio decoder + MF encoder): {:.3}s",
        t_enc_init.elapsed().as_secs_f64()
    );

    let mut frames_encoded: u32 = 0;
    let mut t_encode = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // D3D11-side keyed mutexes for the encode ring — AcquireSync before reading each
    // shared slot and ReleaseSync only after the slot has been copied to a private
    // encode texture owned by this thread.
    let gpu_enc_keyed_mutexes: Vec<IDXGIKeyedMutex> = gpu_buffers
        .iter()
        .map(|b| b.texture.cast::<IDXGIKeyedMutex>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("QI IDXGIKeyedMutex (encode ring): {e}"))?;

    // Pre-cast shared encode textures once for fast CopyResource.
    let gpu_buffer_resources: Vec<ID3D11Resource> = gpu_buffers
        .iter()
        .map(|b| b.texture.cast::<ID3D11Resource>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("QI ID3D11Resource (encode ring): {e}"))?;

    // Encode-side private texture copy context:
    //   shared slot (render-owned) -> private texture (encode-owned) -> MF WriteSample.
    //
    // This decouples shared-slot reuse timing from asynchronous MF consumption and
    // removes ring-periodic frame interleaving when the writer keeps GPU samples alive.
    let (enc_copy_device, enc_copy_context, enc_copy_fence, enc_copy_desc) =
        if let Some(first_buf) = gpu_buffers.first() {
            let device = unsafe {
                first_buf
                    .texture
                    .GetDevice()
                    .map_err(|e| format!("GetDevice (encode shared texture): {e}"))?
            };

            let context = unsafe {
                device
                    .GetImmediateContext()
                    .map_err(|e| format!("GetImmediateContext (encode): {e}"))?
            };

            let fence = D3D11GpuFence::new(&device, &context)?;

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                first_buf.texture.GetDesc(&mut desc);
            }
            desc.MiscFlags = 0;

            (Some(device), Some(context), Some(fence), Some(desc))
        } else {
            (None, None, None, None)
        };

    let mut encode_slow_count: u32 = 0;
    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        let enc_wait_dur = tw0.elapsed().as_secs_f64();
        t_wait += enc_wait_dur;

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let enc_frame_t0 = Instant::now();
        let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;

        // Audio interleaving.
        if let (Some(dec), Some(stream)) = (&audio_decoder, &opt_audio_stream) {
            while !audio_eof && audio_output_100ns <= timestamp_100ns {
                if config.audio_is_preprocessed {
                    match dec.read_samples() {
                        Ok(Some((pcm, _ts_100ns))) => {
                            let channels = dec.channels() as usize;
                            if channels == 0 || pcm.is_empty() {
                                continue;
                            }
                            let samples_per_channel = pcm.len() / (channels * 4);
                            if samples_per_channel == 0 {
                                continue;
                            }
                            let next_total =
                                total_audio_samples_written + samples_per_channel as u64;
                            let next_100ns = (next_total * 10_000_000) / dec.sample_rate() as u64;
                            let dur_100ns = next_100ns as i64 - audio_output_100ns;
                            if dur_100ns <= 0 {
                                continue;
                            }
                            if let Err(e) = stream.write_samples_direct(
                                encoder.writer(),
                                &pcm,
                                audio_output_100ns,
                                dur_100ns,
                            ) {
                                eprintln!("[Audio] Native mixed audio write error: {}", e);
                                audio_eof = true;
                            } else {
                                total_audio_samples_written = next_total;
                                audio_output_100ns = next_100ns as i64;
                            }
                            continue;
                        }
                        Ok(None) => {
                            audio_eof = true;
                            continue;
                        }
                        Err(e) => {
                            eprintln!("[Audio] Native mixed audio decode error: {}", e);
                            audio_eof = true;
                            continue;
                        }
                    }
                }

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
                            let input_frames = if channels == 0 {
                                0
                            } else {
                                pcm.len() / (channels * 4)
                            };
                            let source_duration_sec = if input_frames == 0 {
                                0.0
                            } else {
                                input_frames as f64 / dec.sample_rate() as f64
                            };
                            let mut resampled = resample_pcm_bytes(&pcm, speed, channels);
                            apply_audio_volume_envelope(
                                &mut resampled,
                                chunk_time,
                                source_duration_sec,
                                channels,
                                &config.audio_volume_points,
                            );
                            if channels == 0 || resampled.is_empty() {
                                continue;
                            }
                            let samples_per_channel = resampled.len() / (channels * 4);
                            if samples_per_channel == 0 {
                                continue;
                            }
                            let next_total =
                                total_audio_samples_written + samples_per_channel as u64;
                            let next_100ns = (next_total * 10_000_000) / dec.sample_rate() as u64;
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
                // Acquire D3D11-side keyed mutex — cache-invalidates the DX12-written
                // data so the MF encoder reads the correct frame, not stale L2 data.
                if !gpu_enc_keyed_mutexes.is_empty() {
                    unsafe {
                        gpu_enc_keyed_mutexes[ring_idx]
                            .AcquireSync(0, u32::MAX)
                            .map_err(|e| format!("AcquireSync D3D11 enc[{ring_idx}]: {e}"))?;
                    }
                }

                let private_texture =
                    if let (Some(device), Some(context), Some(fence), Some(desc)) = (
                        &enc_copy_device,
                        &enc_copy_context,
                        &enc_copy_fence,
                        &enc_copy_desc,
                    ) {
                        let mut copied_opt: Option<ID3D11Texture2D> = None;
                        unsafe {
                            device
                                .CreateTexture2D(desc, None, Some(&mut copied_opt))
                                .map_err(|e| format!("CreateTexture2D encode-private: {e}"))?;
                        }
                        let copied =
                            copied_opt.ok_or("CreateTexture2D encode-private returned null")?;

                        let copied_res: ID3D11Resource = copied
                            .cast()
                            .map_err(|e| format!("QI encode-private ID3D11Resource: {e}"))?;
                        unsafe {
                            context.CopyResource(&copied_res, &gpu_buffer_resources[ring_idx]);
                        }
                        // Ensure the shared->private copy is complete before releasing the
                        // shared slot back to the render thread.
                        fence.signal_and_wait();
                        copied
                    } else {
                        gpu_buffers[ring_idx].texture.clone()
                    };

                // Shared slot is no longer needed once the private copy is complete.
                if !gpu_enc_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = gpu_enc_keyed_mutexes[ring_idx].ReleaseSync(0);
                    }
                }
                let _ = gpu_recycle_tx.send(ring_idx);

                encoder.write_frame_gpu(&private_texture, timestamp_100ns, frame_duration_100ns)?;
            }
            RenderOutput::Cpu { rendered_bgra } => {
                encoder.write_frame_cpu(&rendered_bgra, timestamp_100ns, frame_duration_100ns)?;
                let _ = cpu_recycle_tx.send(rendered_bgra);
            }
        }
        t_encode += te0.elapsed().as_secs_f64();

        let enc_frame_dur = enc_frame_t0.elapsed().as_secs_f64();
        if enc_frame_dur > 0.5 || enc_wait_dur > 0.5 {
            encode_slow_count += 1;
            eprintln!(
                "[Encode] SLOW f{}/{total_frames} wait={enc_wait_dur:.3}s encode={enc_frame_dur:.3}s",
                frames_encoded + 1
            );
        }

        frames_encoded += 1;

        if let Some(ref cb) = progress
            && (frames_encoded.is_multiple_of(15) || frames_encoded == total_frames)
        {
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

    encoder.finalize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frames_encoded as f64 / elapsed;
    let n = frames_encoded.max(1) as f64;

    if encode_slow_count > 0 {
        eprintln!("[Encode] {encode_slow_count} slow frames (>0.5s each)");
    }
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
