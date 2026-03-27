// Threaded GPU export pipeline with fully zero-copy decode->render->encode path.
//
// Three threads running in parallel:
//   Decode thread:  MF decode -> D3D11 VP (NV12->BGRA) -> shared VRAM texture -> channel
//   Render thread:  channel -> GPU copy to video texture -> compositor render -> GPU copy to shared -> channel
//   Main thread:    channel -> MF encode -> MP4
//
// Zero-copy path (default):
//   Decode: D3D11 VP blits directly into shared VRAM texture (NT handle), GPU fence, send ring index.
//   Render: wgpu copies shared decode texture to video_texture (GPU-to-GPU), renders, copies output
//           to shared encode texture. No PCIe bus crossings in the entire pipeline.
//   Encode: MF encoder reads directly from shared VRAM via MFCreateDXGISurfaceBuffer.
//
// CPU fallback (env SGT_FORCE_CPU_ENCODE=1 or if shared texture init fails):
//   Decode: D3D11 VP -> CPU readback -> channel (Vec<u8>)
//   Render: CPU upload -> compositor -> [GPU copy | CPU readback] -> channel
//
// Frame selection: sample-and-hold using source PTS to handle VFR sources.
// wgpu (DX12) and D3D11 use completely independent devices -- no D3D11On12.

mod audio;
mod decode_thread;
mod decode_thread_cpu;
mod encode_thread;
mod frame_timing;
mod render_thread;
mod ring_buffers;
mod types;
mod webcam;

pub use frame_timing::build_frame_times;
pub use types::{PipelineConfig, ProgressCallback, ZeroCopyExportResult};

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12 as d3d12;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::Interface;

use super::d3d_interop::{SharedVramBuffer, create_d3d11_device, create_d3d11_device_on_adapter};
use super::gpu_export::{CompositorUniforms, GpuCompositor};
use super::mf_decode::DxgiDeviceManager;

use decode_thread::run_decode_thread;
use decode_thread_cpu::run_decode_thread_cpu;
use encode_thread::run_encode_thread;
use render_thread::run_render_thread;
use ring_buffers::{try_create_decode_input_ring, try_create_gpu_output_ring};
use types::{
    DECODE_RING_SIZE, DecodeOutput, DecodeThreadContext, EncodeThreadContext, GPU_RING_SIZE,
    RenderOutput, RenderThreadContext,
};
use webcam::prepare_webcam_decode_setup;

pub(super) struct ComScope(bool);

impl ComScope {
    pub(super) fn initialize_mta() -> Result<Self, String> {
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

    // --- Device creation (before thread::scope for shared texture init) ---
    //
    // CRITICAL: Create D3D11 devices on the SAME adapter as wgpu (DX12).
    let t_pipeline_setup = Instant::now();
    // On multi-GPU systems (iGPU + dGPU), D3D11CreateDevice(D3D_DRIVER_TYPE_HARDWARE)
    // picks the default adapter which may be the iGPU, while wgpu picks the dGPU.
    // Shared textures between different adapters don't share VRAM -- D3D12 reads
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

    // Decode D3D11 device -- MUST be on the same adapter as wgpu for shared texture coherence.
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

    // Encode D3D11 device -- same adapter requirement.
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

    // --- Zero-copy decode input ring (decode -> render) ---
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

    // --- Zero-copy output ring (render -> encode) ---

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

    // --- Channels ---

    // Forward channels (decode -> render -> encode).
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
        "[Pipeline] {} frames, {}x{} -> {}x{} @ {}fps, decode={}, encode={}, blur={} (z={:.2}, p={:.2}, c={:.2}, mb={}), segs={}",
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

    // Surface decode/render errors -- prefer the earliest root cause over encode errors.
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
