// Media Foundation hardware-accelerated video decoder.
// Decodes video to NV12 D3D11 textures entirely in VRAM (zero CPU copy).

use std::mem::ManuallyDrop;

use base64::Engine as _;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::*;
use windows::Win32::System::Variant::VT_I8;

/// DXGI Device Manager shared between decoder and encoder.
/// Media Foundation requires this to enable HW-accelerated decode/encode.
pub struct DxgiDeviceManager {
    pub manager: IMFDXGIDeviceManager,
    _reset_token: u32,
}

/// A decoded video frame: D3D11 texture + subresource index + timestamp.
///
/// The `sample` field keeps the `IMFSample` alive until this struct drops.
/// Without it, the DXGI surface allocator reclaims the texture subresource
/// as soon as the sample refcount hits zero — even while the VP Blt is
/// still reading from it on the GPU.
pub struct DecodedFrame {
    pub texture: ID3D11Texture2D,
    pub subresource_index: u32,
    /// Presentation timestamp in 100ns units.
    pub pts_100ns: i64,
    /// Keeps the MF sample alive so the DXGI allocator cannot reuse the
    /// texture subresource until after VP Blt + readback complete.
    pub _sample: IMFSample,
}

/// Media Foundation SourceReader for hardware-accelerated video decode.
pub struct MfDecoder {
    reader: IMFSourceReader,
    video_stream_index: u32,
    width: u32,
    height: u32,
}

use std::sync::OnceLock;

static MF_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Initialize Media Foundation runtime. Idempotent — safe to call multiple times.
pub fn mf_startup() -> Result<(), String> {
    MF_INIT
        .get_or_init(|| unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| format!("MFStartup failed: {e}"))
        })
        .clone()
}

/// Shutdown Media Foundation runtime.
/// Intentionally no-op: calling MFShutdown() tears down the shared MF platform
/// used by the WinRT capture pipeline, causing MF_E_SHUTDOWN (0xC00D3E85) on
/// subsequent recordings. The OS reclaims all resources on process exit.
pub fn mf_shutdown() -> Result<(), String> {
    Ok(())
}

impl DxgiDeviceManager {
    /// Create a DXGI Device Manager and associate it with a D3D11 device.
    /// The D3D11 device should be a standalone D3D11 device with VIDEO_SUPPORT.
    pub fn new(d3d11_device: &ID3D11Device) -> Result<Self, String> {
        let mut reset_token: u32 = 0;
        let mut manager: Option<IMFDXGIDeviceManager> = None;

        unsafe {
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
                .map_err(|e| format!("MFCreateDXGIDeviceManager: {e}"))?;
        }

        let manager = manager.ok_or("MFCreateDXGIDeviceManager returned null")?;

        unsafe {
            manager
                .ResetDevice(d3d11_device, reset_token)
                .map_err(|e| format!("ResetDevice: {e}"))?;
        }

        println!("[DxgiDeviceManager] Created with token={reset_token}");

        Ok(Self {
            manager,
            _reset_token: reset_token,
        })
    }
}

impl MfDecoder {
    /// Open a video file for hardware-accelerated decoding.
    ///
    /// The decoder outputs NV12 textures on the same D3D11 device as the manager.
    /// Pass `video_only: true` to disable audio stream selection.
    pub fn new(
        file_path: &str,
        device_manager: &DxgiDeviceManager,
        video_only: bool,
    ) -> Result<Self, String> {
        // Create attributes for the SourceReader
        let attrs = create_reader_attributes(&device_manager.manager)?;

        // Create SourceReader from file
        let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();
        let reader = unsafe {
            MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };

        let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        if video_only {
            // Deselect all streams, then select only video
            let all_streams = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
            unsafe {
                reader
                    .SetStreamSelection(all_streams, false)
                    .map_err(|e| format!("Deselect all: {e}"))?;
                reader
                    .SetStreamSelection(video_idx, true)
                    .map_err(|e| format!("Select video: {e}"))?;
            }
        }

        // Configure output to NV12 (native HW decode format)
        configure_nv12_output(&reader, video_idx)?;

        // Read frame dimensions from the negotiated output type
        let (width, height) = get_frame_size(&reader, video_idx)?;

        println!("[MfDecoder] Opened {}x{}", width, height);

        Ok(Self {
            reader,
            video_stream_index: video_idx,
            width,
            height,
        })
    }

    /// Read the next decoded video frame as a D3D11 texture (NV12 in VRAM).
    ///
    /// Returns `None` at end-of-stream.
    pub fn read_frame(&self) -> Result<Option<DecodedFrame>, String> {
        let mut stream_flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample: Option<IMFSample> = None;

        unsafe {
            self.reader
                .ReadSample(
                    self.video_stream_index,
                    0,
                    None,
                    Some(&mut stream_flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|e| format!("ReadSample: {e}"))?;
        }

        // Check end-of-stream
        if (stream_flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
            return Ok(None);
        }

        let sample = match sample {
            Some(s) => s,
            None => return Ok(None), // No sample but not EOS (e.g., gap)
        };

        // Extract D3D11 texture from the sample's DXGI buffer
        let (texture, subresource_index) = extract_texture_from_sample(&sample)?;

        Ok(Some(DecodedFrame {
            texture,
            subresource_index,
            pts_100ns: timestamp,
            _sample: sample,
        }))
    }

    /// Seek to a position in the video (100ns units).
    pub fn seek(&self, position_100ns: i64) -> Result<(), String> {
        let propvar = make_i64_propvariant(position_100ns);

        unsafe {
            self.reader
                .SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar)
                .map_err(|e| format!("SetCurrentPosition: {e}"))?;
        }

        Ok(())
    }

    /// Seek to a position in seconds.
    pub fn seek_seconds(&self, seconds: f64) -> Result<(), String> {
        let position_100ns = (seconds * 10_000_000.0) as i64;
        self.seek(position_100ns)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Create IMFAttributes for the SourceReader with D3D11 HW acceleration.
fn create_reader_attributes(manager: &IMFDXGIDeviceManager) -> Result<IMFAttributes, String> {
    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 4).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    unsafe {
        // Attach DXGI device manager for HW decode
        attrs
            .SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, manager)
            .map_err(|e| format!("SetUnknown D3D_MANAGER: {e}"))?;

        // Enable hardware transforms (HW decode)
        attrs
            .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
            .map_err(|e| format!("SetUINT32 HW_TRANSFORMS: {e}"))?;

        // Enable advanced video processing (format conversion in reader).
        // NOTE: MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING must NOT be set
        // when MF_SOURCE_READER_D3D_MANAGER is present — they're mutually
        // exclusive per MS docs and combining them returns E_INVALIDARG.
        attrs
            .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
            .map_err(|e| format!("SetUINT32 ADV_VIDEO_PROC: {e}"))?;
    }

    Ok(attrs)
}

/// Configure the SourceReader to output NV12 video.
fn configure_nv12_output(reader: &IMFSourceReader, stream_index: u32) -> Result<(), String> {
    let media_type = unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };

    unsafe {
        // Set major type = Video
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;

        // Set subtype = NV12
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .map_err(|e| format!("SetGUID SUBTYPE NV12: {e}"))?;

        // Tell SourceReader to output this format
        reader
            .SetCurrentMediaType(stream_index, None, &media_type)
            .map_err(|e| format!("SetCurrentMediaType NV12: {e}"))?;
    }

    Ok(())
}

/// Read frame dimensions from the current output media type.
fn get_frame_size(reader: &IMFSourceReader, stream_index: u32) -> Result<(u32, u32), String> {
    let media_type = unsafe {
        reader
            .GetCurrentMediaType(stream_index)
            .map_err(|e| format!("GetCurrentMediaType: {e}"))?
    };

    // MF_MT_FRAME_SIZE is packed as (width << 32) | height in a UINT64
    let frame_size = unsafe {
        media_type
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
    };

    let width = (frame_size >> 32) as u32;
    let height = (frame_size & 0xFFFF_FFFF) as u32;

    Ok((width, height))
}

/// Extract the D3D11 texture from a decoded MF sample.
fn extract_texture_from_sample(sample: &IMFSample) -> Result<(ID3D11Texture2D, u32), String> {
    let buffer = unsafe {
        sample
            .GetBufferByIndex(0)
            .map_err(|e| format!("GetBufferByIndex: {e}"))?
    };

    // QI for IMFDXGIBuffer to get the underlying D3D11 texture
    let dxgi_buffer: IMFDXGIBuffer = buffer
        .cast()
        .map_err(|e| format!("QI IMFDXGIBuffer: {e}"))?;

    // Get the ID3D11Texture2D from the DXGI buffer
    let mut texture_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    unsafe {
        dxgi_buffer
            .GetResource(&ID3D11Texture2D::IID, &mut texture_ptr)
            .map_err(|e| format!("GetResource: {e}"))?;
    }

    if texture_ptr.is_null() {
        return Err("GetResource returned null texture".to_string());
    }

    // Wrap the raw pointer as a COM object (takes ownership of the AddRef'd reference)
    let texture: ID3D11Texture2D = unsafe { ID3D11Texture2D::from_raw(texture_ptr) };

    let subresource_index = unsafe {
        dxgi_buffer
            .GetSubresourceIndex()
            .map_err(|e| format!("GetSubresourceIndex: {e}"))?
    };

    Ok((texture, subresource_index))
}

/// Probe video dimensions using a lightweight MF SourceReader (no GPU decode).
/// Opens the file, reads native media type to get width/height, then drops.
pub fn probe_video_dimensions(file_path: &str) -> Result<(u32, u32), String> {
    let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();

    // Create a bare SourceReader without DXGI device (software probe only)
    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    let reader = unsafe {
        MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
            .map_err(|e| format!("MFCreateSourceReaderFromURL probe: {e}"))?
    };

    let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    // Read the native media type (no decode needed, just metadata)
    let native_type = unsafe {
        reader
            .GetNativeMediaType(video_idx, 0)
            .map_err(|e| format!("GetNativeMediaType: {e}"))?
    };

    let frame_size = unsafe {
        native_type
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
    };

    let width = (frame_size >> 32) as u32;
    let height = (frame_size & 0xFFFF_FFFF) as u32;

    Ok((width, height))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadataProbe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub fps_num: u32,
    pub fps_den: u32,
}

/// Probe video metadata using a lightweight MF SourceReader (no GPU decode).
/// Reads native media type to get frame size + nominal frame rate, then drops.
pub fn probe_video_metadata(file_path: &str) -> Result<VideoMetadataProbe, String> {
    let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();

    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    let reader = unsafe {
        MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
            .map_err(|e| format!("MFCreateSourceReaderFromURL probe: {e}"))?
    };

    let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let native_type = unsafe {
        reader
            .GetNativeMediaType(video_idx, 0)
            .map_err(|e| format!("GetNativeMediaType: {e}"))?
    };

    let frame_size = unsafe {
        native_type
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
    };
    let width = (frame_size >> 32) as u32;
    let height = (frame_size & 0xFFFF_FFFF) as u32;

    let frame_rate = unsafe { native_type.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0) };
    let fps_num = (frame_rate >> 32) as u32;
    let fps_den = (frame_rate & 0xFFFF_FFFF) as u32;
    let fps = if fps_num > 0 && fps_den > 0 {
        fps_num as f64 / fps_den as f64
    } else {
        0.0
    };

    Ok(VideoMetadataProbe {
        width,
        height,
        fps,
        fps_num,
        fps_den,
    })
}

/// Generate lightweight JPEG thumbnails via MF SourceReader in software RGB32 mode.
pub fn generate_thumbnails(
    file_path: &str,
    count: u32,
    start_sec: f64,
    end_sec: f64,
) -> Result<Vec<String>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }

    mf_startup()?;

    let result = (|| -> Result<Vec<String>, String> {
        let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut attrs: Option<IMFAttributes> = None;
        unsafe {
            MFCreateAttributes(&mut attrs, 2).map_err(|e| format!("MFCreateAttributes: {e}"))?;
        }
        let attrs = attrs.ok_or("MFCreateAttributes returned null")?;
        unsafe {
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(|e| format!("SetUINT32 ADVANCED_VIDEO_PROCESSING: {e}"))?;
        }

        let reader = unsafe {
            MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };
        let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        let media_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| format!("SetGUID major type: {e}"))?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|e| format!("SetGUID subtype RGB32: {e}"))?;
            reader
                .SetCurrentMediaType(video_idx, None, &media_type)
                .map_err(|e| format!("SetCurrentMediaType RGB32: {e}"))?;
        }

        let current_type = unsafe {
            reader
                .GetCurrentMediaType(video_idx)
                .map_err(|e| format!("GetCurrentMediaType: {e}"))?
        };
        let frame_size = unsafe {
            current_type
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
        };
        let width = (frame_size >> 32) as u32;
        let height = (frame_size & 0xFFFF_FFFF) as u32;

        let safe_end = if end_sec > start_sec {
            end_sec
        } else {
            start_sec
        };
        let duration = (safe_end - start_sec).max(0.0);
        let step = if count > 1 {
            duration / (count - 1) as f64
        } else {
            0.0
        };

        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let t = start_sec + (i as f64 * step);
            let propvar = make_i64_propvariant((t * 10_000_000.0) as i64);
            unsafe {
                let _ = reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar);
            }

            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                let _ = reader.ReadSample(
                    video_idx,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                );
            }

            if (flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                out.push(String::new());
                continue;
            }

            let Some(sample) = sample else {
                out.push(String::new());
                continue;
            };

            let buffer = unsafe {
                sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| format!("ConvertToContiguousBuffer: {e}"))?
            };
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut length = 0u32;
            unsafe {
                buffer
                    .Lock(&mut data_ptr, None, Some(&mut length))
                    .map_err(|e| format!("IMFMediaBuffer::Lock: {e}"))?;
            }

            let thumb_result = (|| -> Result<String, String> {
                let raw = unsafe { std::slice::from_raw_parts(data_ptr, length as usize) };
                let mut rgba = raw.to_vec();
                for px in rgba.chunks_exact_mut(4) {
                    px.swap(0, 2); // BGRA -> RGBA
                }

                let Some(img) = image::RgbaImage::from_raw(width, height, rgba) else {
                    return Ok(String::new());
                };
                let resized =
                    image::imageops::resize(&img, 160, 90, image::imageops::FilterType::Triangle);
                let mut jpg = Vec::new();
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 60);
                enc.encode_image(&image::DynamicImage::ImageRgba8(resized))
                    .map_err(|e| format!("JpegEncoder: {e}"))?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
                Ok(format!("data:image/jpeg;base64,{b64}"))
            })();

            unsafe {
                let _ = buffer.Unlock();
            }
            out.push(thumb_result.unwrap_or_default());
        }

        Ok(out)
    })();

    let _ = mf_shutdown();
    result
}

/// Create a PROPVARIANT containing an i64 value (VT_I8), used for seeking.
pub(super) fn make_i64_propvariant(value: i64) -> PROPVARIANT {
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { hVal: value },
            }),
        },
    }
}
