use std::sync::mpsc;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12 as d3d12;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;

use super::super::d3d_interop::SharedVramBuffer;
use super::super::mf_encode::VideoCodec;
use super::super::native_export::config::OverlayFrame;
use super::super::native_export::config::{
    AnimatedCursorSlotData, BakedWebcamFrame, DeviceAudioPoint, SpeedPoint, TrimSegment,
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
pub(super) enum DecodeOutput {
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
    pub(super) fn source_time(&self) -> f64 {
        match self {
            Self::Gpu { source_time, .. }
            | Self::GpuHold { source_time, .. }
            | Self::Cpu { source_time, .. }
            | Self::Inactive { source_time, .. } => *source_time,
        }
    }
    pub(super) fn source_step(&self) -> f64 {
        match self {
            Self::Gpu { source_step, .. }
            | Self::GpuHold { source_step, .. }
            | Self::Cpu { source_step, .. }
            | Self::Inactive { source_step, .. } => *source_step,
        }
    }
    pub(super) fn frame_idx(&self) -> u32 {
        match self {
            Self::Gpu { frame_idx, .. }
            | Self::GpuHold { frame_idx, .. }
            | Self::Cpu { frame_idx, .. }
            | Self::Inactive { frame_idx, .. } => *frame_idx,
        }
    }
}

/// Message sent from render thread to encode thread.
pub(super) enum RenderOutput {
    /// CPU path: rendered BGRA pixels (out_w×h×4). Returned to render via recycle.
    Cpu { rendered_bgra: Vec<u8> },
    /// GPU path: index into shared VRAM ring buffer. Returned to render via recycle.
    Gpu { ring_idx: usize },
}

pub(super) const GPU_RING_SIZE: usize = 16;
pub(super) const DECODE_RING_SIZE: usize = 3;

/// Shared VRAM ring for zero-copy render->encode.
pub(super) struct GpuOutputRing {
    pub shared_buffers: Vec<SharedVramBuffer>,
    pub wgpu_textures: Vec<wgpu::Texture>,
    /// Keyed mutex handles for GPU cache coherence between DX12 (render) and D3D11 (encode).
    /// Render thread: AcquireSync before copy, ReleaseSync after poll(Wait).
    /// Encode thread QIs its own set from the same textures.
    pub dx12_keyed_mutexes: Vec<IDXGIKeyedMutex>,
}

/// Shared VRAM ring for zero-copy decode->render.
///
/// Uses D3D11-created `SHARED_KEYEDMUTEX | SHARED_NTHANDLE` textures.
/// Keyed mutex provides CPU-level ownership. Cross-API shared fence provides
/// GPU-timeline ordering (D3D11 Signal -> DX12 Wait). A per-frame 1-pixel
/// `copy_buffer_to_texture` (COPY_DST) before `copy_texture_to_texture` (COPY_SRC)
/// forces a COPY_DST->COPY_SRC DX12 barrier that flushes L2 cache.
///
/// DX12 can't participate in keyed mutex cache coherence (QI for IDXGIKeyedMutex
/// from ID3D12Resource returns E_NOINTERFACE), so we manually force a DX12 barrier
/// via the COPY_DST->COPY_SRC state transition trick.
pub(super) struct DecodeInputRing {
    pub shared_buffers: Vec<SharedVramBuffer>,
    pub wgpu_textures: Vec<wgpu::Texture>,
    pub keyed_mutexes: Vec<IDXGIKeyedMutex>,
    pub d3d11_fence: ID3D11Fence,
    pub d3d12_fence: d3d12::ID3D12Fence,
}

/// GPU decode thread context: VP blits NV12 directly into shared VRAM textures (zero-copy).
///
/// Keeps `cur_decoded`/`next_decoded` DecodedFrames alive for the "hold" case.
/// Per output frame we VP Blt the current NV12 source into a shared ring texture,
/// GPU fence, then send the ring index to the render thread. Re-VP-Blt for held
/// frames is cheap (microseconds on GPU) compared to the old CPU readback (~6ms).
pub(super) struct DecodeThreadContext<'a> {
    pub label: &'static str,
    pub cancel_flag: &'a std::sync::atomic::AtomicBool,
    pub source_video_path: &'a str,
    pub source_times: &'a [f64],
    pub speed_points: &'a [SpeedPoint],
    pub trim_segments: &'a [TrimSegment],
    pub framerate: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub source_rect_width: u32,
    pub source_rect_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub active_mask: Option<&'a [bool]>,
    pub d3d11_device: &'a ID3D11Device,
    pub d3d11_context: &'a ID3D11DeviceContext,
    pub tx: mpsc::SyncSender<DecodeOutput>,
    pub recycle_rx: mpsc::Receiver<usize>,
    pub shared_textures: &'a [ID3D11Texture2D],
    pub keyed_mutexes: &'a [IDXGIKeyedMutex],
    pub d3d11_fence: Option<&'a ID3D11Fence>,
}

/// Render thread context: receives decoded frames, runs compositor, sends output to encoder.
///
/// Decode input: GPU path copies shared decode texture to video_texture (fast GPU copy).
///               CPU path uploads BGRA via queue.write_texture (PCIe upload).
/// Encode output: GPU path copies output to shared VRAM texture, send ring_idx.
///                CPU path: pipelined readback depth 2, send BGRA Vec.
pub(super) struct RenderThreadContext<'a> {
    pub config: &'a PipelineConfig,
    pub compositor: &'a mut super::super::gpu_export::GpuCompositor,
    pub build_uniforms:
        &'a (dyn Fn(f64, f64, f64, f64) -> super::super::gpu_export::CompositorUniforms + Sync),
    pub cancel_flag: &'a std::sync::atomic::AtomicBool,
    pub rx: mpsc::Receiver<DecodeOutput>,
    pub mb_samples: u32,
    pub tx: mpsc::SyncSender<RenderOutput>,
    // Decode recycle channels (render -> decode).
    pub dec_gpu_recycle_tx: mpsc::Sender<usize>,
    pub dec_cpu_recycle_tx: mpsc::Sender<Vec<u8>>,
    pub dec_wgpu_textures: &'a [wgpu::Texture],
    // Cross-API shared fence: DX12 waits for D3D11's Signal before reading shared textures.
    pub dec_d3d12_fence: Option<&'a d3d12::ID3D12Fence>,
    // Keyed mutexes for decode ring slots (CPU ownership protocol).
    pub dec_keyed_mutexes: &'a [IDXGIKeyedMutex],
    pub webcam_rx: Option<mpsc::Receiver<DecodeOutput>>,
    pub webcam_gpu_recycle_tx: Option<mpsc::Sender<usize>>,
    pub webcam_wgpu_textures: &'a [wgpu::Texture],
    pub webcam_d3d12_fence: Option<&'a d3d12::ID3D12Fence>,
    pub webcam_keyed_mutexes: &'a [IDXGIKeyedMutex],
    pub webcam_render_width: u32,
    pub webcam_render_height: u32,
    // Encode output resources.
    pub gpu_textures: &'a [wgpu::Texture],
    // D3D12-side keyed mutex per encode ring slot.
    // AcquireSync before writing; ReleaseSync after poll(Wait)+on_submitted_work_done.
    pub gpu_dx12_keyed_mutexes: &'a [IDXGIKeyedMutex],
    pub gpu_slot_rx: mpsc::Receiver<usize>,
    pub cpu_recycle_rx: mpsc::Receiver<Vec<u8>>,
}

/// Encode thread context: receives rendered frames, interleaves audio, encodes to MP4.
///
/// GPU path: reads directly from shared VRAM textures via MFCreateDXGISurfaceBuffer.
/// CPU path: receives BGRA Vec and uses MFCreateMemoryBuffer (original path).
pub(super) struct EncodeThreadContext<'a> {
    pub config: &'a PipelineConfig,
    pub enc_device_manager: &'a super::super::mf_decode::DxgiDeviceManager,
    pub progress: Option<ProgressCallback>,
    pub cancel_flag: &'a std::sync::atomic::AtomicBool,
    pub rx: &'a mpsc::Receiver<RenderOutput>,
    pub total_frames: u32,
    pub start: &'a std::time::Instant,
    pub gpu_buffers: &'a [SharedVramBuffer],
    pub gpu_recycle_tx: mpsc::Sender<usize>,
    pub cpu_recycle_tx: mpsc::Sender<Vec<u8>>,
}

/// Webcam decode setup returned by prepare_webcam_decode_setup.
pub(super) struct WebcamDecodeSetup {
    pub d3d_device: ID3D11Device,
    pub d3d_context: ID3D11DeviceContext,
    pub ring: DecodeInputRing,
    pub source_times: Vec<f64>,
    pub active_mask: Vec<bool>,
    pub source_width: u32,
    pub source_height: u32,
    pub render_width: u32,
    pub render_height: u32,
}
