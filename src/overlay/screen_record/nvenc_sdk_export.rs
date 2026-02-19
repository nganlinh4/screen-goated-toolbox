use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};

#[derive(Clone, Copy, Debug)]
pub enum NvencSdkProfile {
    Turbo,
    MaxSpeed,
    Balanced,
    QualityStrict,
}

#[derive(Clone, Copy, Debug)]
pub enum NvencSdkCodec {
    H264,
    Hevc,
}

#[derive(Clone, Debug)]
pub struct NvencSdkSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub target_bitrate_kbps: u32,
    pub profile: NvencSdkProfile,
    pub codec: NvencSdkCodec,
    pub output_stream_path: PathBuf,
}

fn parse_bool_env(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn should_attempt_sdk_lane() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }

    if parse_bool_env("SR_NVENC_SDK_DISABLE").unwrap_or(false) {
        return false;
    }

    if let Some(explicit) = parse_bool_env("SR_NVENC_SDK_ENABLE") {
        return explicit;
    }

    if let Some(legacy) = parse_bool_env("SR_NVENC_SDK_EXPERIMENTAL") {
        return legacy;
    }

    // Default ON: caller can disable with SR_NVENC_SDK_DISABLE=1.
    true
}

pub fn should_attempt_zero_copy_lane() -> bool {
    if !should_attempt_sdk_lane() {
        return false;
    }

    if parse_bool_env("SR_NVENC_SDK_ZERO_COPY_DISABLE").unwrap_or(false) {
        return false;
    }

    if let Some(explicit) = parse_bool_env("SR_NVENC_SDK_ZERO_COPY_ENABLE") {
        return explicit;
    }

    // Default ON when SDK lane is enabled.
    true
}

pub fn codec_ffmpeg_name(codec: NvencSdkCodec) -> &'static str {
    match codec {
        NvencSdkCodec::H264 => "h264",
        NvencSdkCodec::Hevc => "hevc",
    }
}

pub fn codec_label(codec: NvencSdkCodec) -> &'static str {
    match codec {
        NvencSdkCodec::H264 => "h264_nvenc_sdk",
        NvencSdkCodec::Hevc => "hevc_nvenc_sdk",
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use nvenc::bitstream::BitStream;
    use nvenc::encoder::Encoder;
    use nvenc::session::{InitParams, NeedsConfig, Session};
    use nvenc::sys::enums::{
        NVencBufferFormat, NVencBufferUsage, NVencInputResourceType, NVencMemoryHeap,
        NVencParamsRcMode, NVencPicStruct, NVencPicType, NVencTuningInfo,
    };
    use nvenc::sys::guids::{
        NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID, NV_ENC_PRESET_P1_GUID,
        NV_ENC_PRESET_P2_GUID, NV_ENC_PRESET_P5_GUID,
    };
    use nvenc::sys::structs::{
        NV_ENC_FENCE_POINT_D3D12_VER, NV_ENC_INPUT_RESOURCE_D3D12_VER,
        NVencFencePointD3D12, NVencInputResourceD3D12,
    };
    use windows::core::Interface;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    };
    use windows::Win32::Graphics::Direct3D12::{
        D3D12_FENCE_FLAG_NONE, ID3D12CommandQueue, ID3D12Device, ID3D12Fence, ID3D12Resource,
    };

    const NVENC_D3D12_FENCE_WAIT: u32 = 1;

    pub struct NvencSdkZeroCopySession {
        encoder: Encoder,
        input_format: NVencBufferFormat,
        input_registered: Option<*mut c_void>,
        input_mapped: Option<*mut c_void>,
        bitstream: BitStream,
        input_resource: NVencInputResourceD3D12,
        render_queue: Option<ID3D12CommandQueue>,
        render_fence: Option<ID3D12Fence>,
        render_fence_value: u64,
        require_gpu_idle_wait: bool,
        out: File,
        frame_idx: usize,
        frame_time_step_us: u64,
        width: u32,
        height: u32,
    }

    impl Drop for NvencSdkZeroCopySession {
        fn drop(&mut self) {
            self.release_output_resources();
        }
    }

    unsafe fn clone_com_from_raw<T: Interface>(raw: *mut c_void, label: &str) -> Result<T, String> {
        if raw.is_null() {
            return Err(format!("{} pointer is null", label));
        }
        let raw_ptr = raw;
        let borrowed = unsafe { T::from_raw_borrowed(&raw_ptr) }
            .ok_or_else(|| format!("{} pointer is invalid", label))?;
        Ok(borrowed.clone())
    }

    fn empty_fence_point() -> NVencFencePointD3D12 {
        NVencFencePointD3D12 {
            version: NV_ENC_FENCE_POINT_D3D12_VER,
            rsvd: 0,
            p_fence: std::ptr::null_mut(),
            wait_value: 0,
            signal_value: 0,
            bit_flags: 0,
            rsvd1: [0; 7],
        }
    }


    fn create_d3d11_device() -> Result<ID3D11Device, String> {
        let mut device = None;
        let mut device_context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut device_context),
            )
            .map_err(|e| format!("D3D11CreateDevice failed: {}", e))?;
        }
        device.ok_or_else(|| "D3D11 device is None".to_string())
    }

    fn select_codec(
        session: &Session<NeedsConfig>,
        prefer_hevc: bool,
    ) -> Result<NvencSdkCodec, String> {
        let codecs = session
            .get_encode_codecs()
            .map_err(|e| format!("NVENC get codecs failed: {:?}", e))?;
        let has_h264 = codecs.contains(&NV_ENC_CODEC_H264_GUID);
        let has_hevc = codecs.contains(&NV_ENC_CODEC_HEVC_GUID);

        if prefer_hevc && has_hevc {
            return Ok(NvencSdkCodec::Hevc);
        }
        if has_h264 {
            return Ok(NvencSdkCodec::H264);
        }
        if has_hevc {
            return Ok(NvencSdkCodec::Hevc);
        }

        Err("No supported NVENC codec (H264/HEVC)".to_string())
    }

    fn init_encoder_with_format(
        session: Session<NeedsConfig>,
        settings: &NvencSdkSettings,
        buffer_format: NVencBufferFormat,
        enable_output_in_vidmem: bool,
    ) -> Result<Encoder, String> {
        let (session, mut preset_cfg) = session
            .get_encode_preset_config_ex(
                match settings.codec {
                    NvencSdkCodec::H264 => NV_ENC_CODEC_H264_GUID,
                    NvencSdkCodec::Hevc => NV_ENC_CODEC_HEVC_GUID,
                },
                match settings.profile {
                    NvencSdkProfile::Turbo | NvencSdkProfile::MaxSpeed => NV_ENC_PRESET_P1_GUID,
                    NvencSdkProfile::Balanced => NV_ENC_PRESET_P2_GUID,
                    NvencSdkProfile::QualityStrict => NV_ENC_PRESET_P5_GUID,
                },
                match settings.profile {
                    NvencSdkProfile::Turbo | NvencSdkProfile::MaxSpeed => {
                        NVencTuningInfo::UltraLowLatency
                    }
                    NvencSdkProfile::Balanced => NVencTuningInfo::LowLatency,
                    NvencSdkProfile::QualityStrict => NVencTuningInfo::HighQuality,
                },
            )
            .map_err(|e| format!("NVENC preset config failed: {:?}", e))?;

        preset_cfg.preset_cfg.rc_params.rate_control_mode = NVencParamsRcMode::VBR;
        preset_cfg.preset_cfg.rc_params.average_bit_rate = settings.target_bitrate_kbps * 1000;
        preset_cfg.preset_cfg.rc_params.look_ahead_depth = match settings.profile {
            NvencSdkProfile::QualityStrict => 16,
            _ => 0,
        };
        preset_cfg.preset_cfg.gop_len = (settings.fps.max(24) * 2).max(48);
        preset_cfg.preset_cfg.frame_interval_p = match settings.profile {
            NvencSdkProfile::QualityStrict => 2,
            _ => 1,
        };

        let init_params = InitParams {
            encode_guid: match settings.codec {
                NvencSdkCodec::H264 => NV_ENC_CODEC_H264_GUID,
                NvencSdkCodec::Hevc => NV_ENC_CODEC_HEVC_GUID,
            },
            preset_guid: match settings.profile {
                NvencSdkProfile::Turbo | NvencSdkProfile::MaxSpeed => NV_ENC_PRESET_P1_GUID,
                NvencSdkProfile::Balanced => NV_ENC_PRESET_P2_GUID,
                NvencSdkProfile::QualityStrict => NV_ENC_PRESET_P5_GUID,
            },
            resolution: [settings.width, settings.height],
            aspect_ratio: [settings.width.max(1), settings.height.max(1)],
            frame_rate: [settings.fps.max(1), 1],
            tuning_info: match settings.profile {
                NvencSdkProfile::Turbo | NvencSdkProfile::MaxSpeed => {
                    NVencTuningInfo::UltraLowLatency
                }
                NvencSdkProfile::Balanced => NVencTuningInfo::LowLatency,
                NvencSdkProfile::QualityStrict => NVencTuningInfo::HighQuality,
            },
            buffer_format,
            encode_config: &mut preset_cfg.preset_cfg,
            enable_ptd: true,
            enable_output_in_vidmem,
            max_encoder_resolution: [settings.width, settings.height],
        };
        session
            .init_encoder(init_params)
            .map_err(|e| format!("NVENC init encoder failed: {:?}", e))
    }

    pub fn preflight(prefer_hevc: bool) -> Result<NvencSdkCodec, String> {
        let device = create_d3d11_device()?;
        let session: Session<NeedsConfig> =
            Session::open_dx(&device).map_err(|e| format!("NVENC open session failed: {:?}", e))?;
        select_codec(&session, prefer_hevc)
    }

    pub fn encode_frames_to_file(
        frame_rx: Receiver<Vec<u8>>,
        recycle_tx: SyncSender<Vec<u8>>,
        settings: NvencSdkSettings,
    ) -> Result<(), String> {
        let device = create_d3d11_device()?;
        let session: Session<NeedsConfig> =
            Session::open_dx(&device).map_err(|e| format!("NVENC open session failed: {:?}", e))?;
        let encoder =
            init_encoder_with_format(session, &settings, NVencBufferFormat::ARGB, false)?;

        let input = encoder
            .create_input_buffer(
                settings.width,
                settings.height,
                NVencMemoryHeap::SystemCached,
                NVencBufferFormat::ARGB,
            )
            .map_err(|e| format!("NVENC create input buffer failed: {:?}", e))?;
        let bitstream = encoder
            .create_bitstream_buffer()
            .map_err(|e| format!("NVENC create bitstream failed: {:?}", e))?;

        let mut out = File::create(&settings.output_stream_path)
            .map_err(|e| format!("Create NVENC stream file failed: {}", e))?;
        let src_stride = (settings.width as usize) * 4;
        let required_bytes = src_stride * settings.height as usize;
        let mut frame_idx = 0usize;
        let frame_time_step_us = 1_000_000u64 / settings.fps.max(1) as u64;

        while let Ok(mut frame) = frame_rx.recv() {
            if frame.len() < required_bytes {
                return Err(format!(
                    "NVENC frame too small: got {}, need {}",
                    frame.len(),
                    required_bytes
                ));
            }

            {
                let lock = input
                    .lock()
                    .map_err(|e| format!("NVENC lock input failed: {:?}", e))?;
                let dst_ptr = unsafe { lock.data_ptr() };
                let pitch = lock.pitch() as usize;
                for y in 0..settings.height as usize {
                    let src_off = y * src_stride;
                    let dst_off = y * pitch;
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            frame.as_ptr().add(src_off),
                            dst_ptr.add(dst_off),
                            src_stride,
                        );
                    }
                }
            }

            encoder
                .encode_picture(
                    &input,
                    &bitstream,
                    frame_idx,
                    frame_idx as u64 * frame_time_step_us,
                    NVencBufferFormat::ARGB,
                    NVencPicStruct::Frame,
                    NVencPicType::UNKNOWN,
                    None,
                )
                .map_err(|e| format!("NVENC encode picture failed: {:?}", e))?;

            {
                let lock = bitstream
                    .try_lock(true)
                    .map_err(|e| format!("NVENC lock bitstream failed: {:?}", e))?;
                out.write_all(lock.as_slice())
                    .map_err(|e| format!("Write NVENC bitstream failed: {}", e))?;
            }

            frame.clear();
            let _ = recycle_tx.try_send(frame);
            frame_idx += 1;
        }

        encoder
            .end_encode()
            .map_err(|e| format!("NVENC end encode failed: {:?}", e))?;

        for _ in 0..4 {
            match bitstream.try_lock(true) {
                Ok(lock) => {
                    if lock.as_slice().is_empty() {
                        break;
                    }
                    out.write_all(lock.as_slice())
                        .map_err(|e| format!("Write NVENC tail bitstream failed: {}", e))?;
                }
                Err(_) => break,
            }
        }

        out.flush()
            .map_err(|e| format!("Flush NVENC stream failed: {}", e))?;
        Ok(())
    }

    pub fn begin_zero_copy_dx12(
        device: *mut c_void,
        queue: *mut c_void,
        texture: *mut c_void,
        settings: NvencSdkSettings,
    ) -> Result<NvencSdkZeroCopySession, String> {
        let session: Session<NeedsConfig> = unsafe { Session::open_dx_raw(device) }
            .map_err(|e| format!("NVENC open session failed: {:?}", e))?;
        // wgpu output texture is BGRA; NVENC uses ARGB for that byte layout.
        let input_format = NVencBufferFormat::ARGB;
        let encoder = init_encoder_with_format(session, &settings, input_format, false)?;
        println!("[Export][SDK] zero-copy DX12 mapped input + host bitstream output");
        let dx12_device = unsafe { clone_com_from_raw::<ID3D12Device>(device, "d3d12_device") }?;
        let dx12_input_resource =
            unsafe { clone_com_from_raw::<ID3D12Resource>(texture, "d3d12_texture") }?;
        let input_desc = unsafe { dx12_input_resource.GetDesc() };
        println!(
            "[Export][SDK] DX12 input desc: dim={:?} fmt={:?} size={}x{} mips={} samples={} flags={:?} layout={:?} input_fmt=0x{:08x}",
            input_desc.Dimension,
            input_desc.Format,
            input_desc.Width,
            input_desc.Height,
            input_desc.MipLevels,
            input_desc.SampleDesc.Count,
            input_desc.Flags,
            input_desc.Layout,
            input_format as u32
        );
        let render_queue = unsafe { clone_com_from_raw::<ID3D12CommandQueue>(queue, "d3d12_queue") }
            .ok();
        let mut render_fence = None;
        if render_queue.is_some() {
            match unsafe { dx12_device.CreateFence::<ID3D12Fence>(0, D3D12_FENCE_FLAG_NONE) } {
                Ok(fence) => render_fence = Some(fence),
                Err(err) => {
                    println!(
                        "[Export][SDK] CreateFence failed ({}), using device idle waits for DX12 sync",
                        err
                    );
                }
            }
        }
        let register_pitches = [0u32, settings.width.saturating_mul(4)];
        let mut register_errors = Vec::new();
        let mut input_registered = None;
        for pitch in register_pitches {
            if input_registered.is_some() {
                break;
            }
            println!(
                "[Export][SDK] register-input: fmt=0x{:08x} pitch={} fence={}",
                input_format as u32,
                pitch,
                render_fence.is_some()
            );
            match unsafe {
                encoder.register_resource_raw_with_fence(
                    texture,
                    input_format,
                    NVencBufferUsage::Image,
                    NVencInputResourceType::DirectX,
                    [settings.width, settings.height],
                    pitch,
                    None,
                )
            } {
                Ok(registered) => input_registered = Some(registered),
                Err(err) => register_errors.push(format!("pitch{}:{:?}", pitch, err)),
            }
        }
        let input_registered = input_registered.ok_or_else(|| {
            format!(
                "NVENC register DX12 texture failed: {}",
                register_errors.join("|")
            )
        })?;
        let mapped_input = match unsafe { encoder.map_input_resource_raw(input_registered, input_format) } {
            Ok(mapped) => mapped,
            Err(err) => {
                let _ = unsafe { encoder.unregister_resource_raw(input_registered) };
                return Err(format!("NVENC map DX12 texture failed: {:?}", err));
            }
        };
        let bitstream = encoder
            .create_bitstream_buffer()
            .map_err(|e| {
                let _ = unsafe { encoder.unmap_input_resource_raw(mapped_input) };
                let _ = unsafe { encoder.unregister_resource_raw(input_registered) };
                format!("NVENC create bitstream failed: {:?}", e)
            })?;
        let mut input_fence = empty_fence_point();
        if let Some(fence) = render_fence.as_ref() {
            input_fence.p_fence = fence.as_raw();
            input_fence.bit_flags = NVENC_D3D12_FENCE_WAIT;
        }
        let require_gpu_idle_wait = queue.is_null() || render_fence.is_none();

        let out = File::create(&settings.output_stream_path)
            .map_err(|e| format!("Create NVENC stream file failed: {}", e))?;
        Ok(NvencSdkZeroCopySession {
            encoder,
            input_format,
            input_registered: Some(input_registered),
            input_mapped: Some(mapped_input),
            bitstream,
            input_resource: NVencInputResourceD3D12 {
                version: NV_ENC_INPUT_RESOURCE_D3D12_VER,
                rsvd: 0,
                input_buffer: mapped_input,
                input_fence_point: input_fence,
                rsvd1: [0; 16],
                rsvd2: [std::ptr::null_mut(); 16],
            },
            render_queue,
            render_fence,
            render_fence_value: 0,
            require_gpu_idle_wait,
            out,
            frame_idx: 0,
            frame_time_step_us: 1_000_000u64 / settings.fps.max(1) as u64,
            width: settings.width,
            height: settings.height,
        })
    }

    impl NvencSdkZeroCopySession {
        fn release_output_resources(&mut self) {
            if let Some(mapped) = self.input_mapped.take() {
                let _ = unsafe { self.encoder.unmap_input_resource_raw(mapped) };
            }
            if let Some(registered) = self.input_registered.take() {
                let _ = unsafe { self.encoder.unregister_resource_raw(registered) };
            }
        }

        pub fn requires_gpu_idle_wait(&self) -> bool {
            self.require_gpu_idle_wait
        }

        pub fn encode_frame(&mut self) -> Result<(), String> {
            if let (Some(queue), Some(fence)) = (self.render_queue.as_ref(), self.render_fence.as_ref())
            {
                self.render_fence_value = self.render_fence_value.saturating_add(1);
                unsafe {
                    queue
                        .Signal(fence, self.render_fence_value)
                        .map_err(|e| format!("DX12 queue signal failed: {}", e))?;
                }
                self.input_resource.input_fence_point.p_fence = fence.as_raw();
                self.input_resource.input_fence_point.wait_value = self.render_fence_value;
                self.input_resource.input_fence_point.signal_value = 0;
                self.input_resource.input_fence_point.bit_flags = NVENC_D3D12_FENCE_WAIT;
            } else {
                self.input_resource.input_fence_point.p_fence = std::ptr::null_mut();
                self.input_resource.input_fence_point.wait_value = 0;
                self.input_resource.input_fence_point.signal_value = 0;
                self.input_resource.input_fence_point.bit_flags = 0;
            }

            let input_ptr = (&mut self.input_resource as *mut NVencInputResourceD3D12).cast::<c_void>();
            let output_ptr = self.bitstream.raw_ptr();
            self.encoder
                .encode_picture_raw(
                    input_ptr,
                    self.width,
                    self.height,
                    self.width,
                    output_ptr,
                    self.frame_idx,
                    self.frame_idx as u64 * self.frame_time_step_us,
                    self.input_format,
                    NVencPicStruct::Frame,
                    NVencPicType::UNKNOWN,
                    None,
                )
                .map_err(|e| format!("NVENC zero-copy encode failed: {:?}", e))?;

            {
                let lock = self
                    .bitstream
                    .try_lock(true)
                    .map_err(|e| format!("NVENC lock bitstream failed: {:?}", e))?;
                self.out
                    .write_all(lock.as_slice())
                    .map_err(|e| format!("Write NVENC bitstream failed: {}", e))?;
            }

            self.frame_idx += 1;
            Ok(())
        }

        pub fn finish(mut self) -> Result<(), String> {
            self.encoder
                .end_encode()
                .map_err(|e| format!("NVENC end encode failed: {:?}", e))?;
            for _ in 0..4 {
                match self.bitstream.try_lock(true) {
                    Ok(lock) => {
                        if lock.as_slice().is_empty() {
                            break;
                        }
                        self.out
                            .write_all(lock.as_slice())
                            .map_err(|e| format!("Write NVENC tail bitstream failed: {}", e))?;
                    }
                    Err(_) => break,
                }
            }
            let flush_result = self.out
                .flush()
                .map_err(|e| format!("Flush NVENC stream failed: {}", e));
            self.release_output_resources();
            flush_result
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;

    pub fn preflight(_prefer_hevc: bool) -> Result<NvencSdkCodec, String> {
        Err("nvenc_sdk_windows_only".to_string())
    }

    pub fn encode_frames_to_file(
        _frame_rx: Receiver<Vec<u8>>,
        _recycle_tx: SyncSender<Vec<u8>>,
        _settings: NvencSdkSettings,
    ) -> Result<(), String> {
        Err("nvenc_sdk_windows_only".to_string())
    }
}

pub use imp::{encode_frames_to_file, preflight};
#[cfg(target_os = "windows")]
pub use imp::{begin_zero_copy_dx12, NvencSdkZeroCopySession};
