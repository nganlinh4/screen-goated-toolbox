// D3D11 device creation, video processing, CPU readback, and shared VRAM utilities.
// Creates a standalone D3D11 device for Media Foundation decode/encode
// and D3D11 VideoProcessor for NV12→RGBA color space conversion.
//
// wgpu (DX12) and D3D11 are completely independent devices — no D3D11On12.
// SharedVramBuffer enables GPU-to-GPU transfer: wgpu renders → copy to shared
// D3D11 texture → MF encoder reads directly, eliminating PCIe round-trips.

use std::mem::ManuallyDrop;

use windows::core::Interface;
use windows::Graphics::DirectX::Direct3D11::IDirect3DSurface;
use windows::Win32::Foundation::{HANDLE, HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::{IDXGIResource1, IDXGISurface};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11SurfaceFromDXGISurface;

/// Create a standalone D3D11 device with video processing support.
///
/// Returns (device, immediate_context). The device supports D3D11 VideoProcessor
/// and can be used with MF DXGI Device Manager for hardware decode/encode.
pub fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let feature_levels = [D3D_FEATURE_LEVEL_11_0];
    let flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

    let try_create = |driver_type| {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let result = unsafe {
            D3D11CreateDevice(
                None,
                driver_type,
                HMODULE::default(),
                flags,
                Some(&feature_levels),
                7, // D3D11_SDK_VERSION
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        result.map(|_| (device.unwrap(), context.unwrap()))
    };

    let (device, context) = try_create(D3D_DRIVER_TYPE_HARDWARE)
        .or_else(|hw_err| {
            eprintln!("[D3D11] Hardware device failed ({hw_err}), retrying with WARP");
            try_create(D3D_DRIVER_TYPE_WARP)
        })
        .map_err(|e| format!("D3D11CreateDevice (hw+warp): {e}"))?;

    println!("[D3D11] Standalone device created with VIDEO_SUPPORT + BGRA_SUPPORT");
    Ok((device, context))
}

/// Staging texture for CPU readback of D3D11 GPU textures.
///
/// Creates a D3D11_USAGE_STAGING texture matching the source dimensions.
/// CopyResource transfers GPU→staging, then Map/Unmap reads to CPU.
pub struct D3D11Readback {
    staging: ID3D11Texture2D,
    context: ID3D11DeviceContext,
    width: u32,
    height: u32,
}

impl D3D11Readback {
    /// Create a readback helper for textures of the given dimensions and format.
    pub fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<Self, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };

        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe {
            device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("CreateTexture2D staging: {e}"))?;
        }
        let staging = texture.ok_or("CreateTexture2D staging returned null")?;

        Ok(Self {
            staging,
            context: context.clone(),
            width,
            height,
        })
    }

    /// Copy a D3D11 texture to the staging texture and read RGBA data to a Vec.
    ///
    /// The source texture must have the same dimensions and format as this readback.
    /// Row padding is stripped — output is tightly packed (width * 4 bytes per row).
    pub fn readback(&self, source: &ID3D11Texture2D, buf: &mut Vec<u8>) -> Result<(), String> {
        let source_res: ID3D11Resource = source
            .cast()
            .map_err(|e| format!("source→ID3D11Resource: {e}"))?;
        let staging_res: ID3D11Resource = self
            .staging
            .cast()
            .map_err(|e| format!("staging→ID3D11Resource: {e}"))?;

        unsafe {
            self.context.CopyResource(&staging_res, &source_res);
            self.context.Flush();
        }

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging_res, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| format!("Map staging: {e}"))?;
        }

        let row_pitch = mapped.RowPitch as usize;
        let row_bytes = (self.width * 4) as usize;
        let total_bytes = row_bytes * self.height as usize;

        buf.clear();
        buf.reserve(total_bytes);

        unsafe {
            let base_ptr = mapped.pData as *const u8;
            if row_pitch == row_bytes {
                let slice = std::slice::from_raw_parts(base_ptr, total_bytes);
                buf.extend_from_slice(slice);
            } else {
                for y in 0..self.height as usize {
                    let row_ptr = base_ptr.add(y * row_pitch);
                    let row = std::slice::from_raw_parts(row_ptr, row_bytes);
                    buf.extend_from_slice(row);
                }
            }
        }

        unsafe {
            self.context.Unmap(&staging_res, 0);
        }

        Ok(())
    }
}

/// D3D11 Video Processor for NV12↔RGBA/BGRA color space conversion on GPU.
pub struct VideoProcessor {
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
}

impl VideoProcessor {
    /// Create a D3D11 Video Processor for NV12↔RGBA conversion.
    pub fn new(
        d3d11_device: &ID3D11Device,
        d3d11_context: &ID3D11DeviceContext,
        input_w: u32,
        input_h: u32,
        output_w: u32,
        output_h: u32,
    ) -> Result<Self, String> {
        let video_device: ID3D11VideoDevice = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11VideoDevice: {e}"))?;

        let video_context: ID3D11VideoContext = d3d11_context
            .cast()
            .map_err(|e| format!("QI ID3D11VideoContext: {e}"))?;

        let content_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            InputWidth: input_w,
            InputHeight: input_h,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            OutputWidth: output_w,
            OutputHeight: output_h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };

        let enumerator = unsafe {
            video_device
                .CreateVideoProcessorEnumerator(&content_desc)
                .map_err(|e| format!("CreateVideoProcessorEnumerator: {e}"))?
        };

        let processor = unsafe {
            video_device
                .CreateVideoProcessor(&enumerator, 0)
                .map_err(|e| format!("CreateVideoProcessor: {e}"))?
        };

        println!(
            "[VideoProcessor] Created {}x{} → {}x{} converter",
            input_w, input_h, output_w, output_h
        );

        Ok(Self {
            processor,
            enumerator,
            video_device,
            video_context,
        })
    }

    /// Set the source rectangle (crop region from input coordinates).
    ///
    /// When set, the VP reads only this rectangle from the input texture
    /// and scales it to fill the entire output. Use for hardware-accelerated crop.
    pub fn set_source_rect(&self, x: u32, y: u32, w: u32, h: u32) {
        let rect = RECT {
            left: x as i32,
            top: y as i32,
            right: (x + w) as i32,
            bottom: (y + h) as i32,
        };
        unsafe {
            self.video_context.VideoProcessorSetStreamSourceRect(
                &self.processor,
                0,
                true,
                Some(&rect),
            );
        }
    }

    /// Convert a D3D11 texture from one format to another.
    pub fn convert(
        &self,
        input_texture: &ID3D11Texture2D,
        input_subresource: u32,
        output_texture: &ID3D11Texture2D,
    ) -> Result<(), String> {
        let input_view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: input_subresource,
                },
            },
        };
        let input_resource: ID3D11Resource = input_texture
            .cast()
            .map_err(|e| format!("Input cast: {e}"))?;

        let mut input_view: Option<ID3D11VideoProcessorInputView> = None;
        unsafe {
            self.video_device
                .CreateVideoProcessorInputView(
                    &input_resource,
                    &self.enumerator,
                    &input_view_desc,
                    Some(&mut input_view),
                )
                .map_err(|e| format!("CreateInputView: {e}"))?;
        }
        let input_view = input_view.ok_or("CreateInputView returned null")?;

        let output_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let output_resource: ID3D11Resource = output_texture
            .cast()
            .map_err(|e| format!("Output cast: {e}"))?;

        let mut output_view: Option<ID3D11VideoProcessorOutputView> = None;
        unsafe {
            self.video_device
                .CreateVideoProcessorOutputView(
                    &output_resource,
                    &self.enumerator,
                    &output_view_desc,
                    Some(&mut output_view),
                )
                .map_err(|e| format!("CreateOutputView: {e}"))?;
        }
        let output_view = output_view.ok_or("CreateOutputView returned null")?;

        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            ppPastSurfaces: std::ptr::null_mut(),
            pInputSurface: ManuallyDrop::new(Some(input_view)),
            ppFutureSurfaces: std::ptr::null_mut(),
            ppPastSurfacesRight: std::ptr::null_mut(),
            pInputSurfaceRight: ManuallyDrop::new(None),
            ppFutureSurfacesRight: std::ptr::null_mut(),
        };

        unsafe {
            self.video_context
                .VideoProcessorBlt(&self.processor, &output_view, 0, &[stream])
                .map_err(|e| format!("VideoProcessorBlt: {e}"))?;
        }

        Ok(())
    }

    /// Create a standalone D3D11 texture.
    pub fn create_texture(
        d3d11_device: &ID3D11Device,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        bind_flags: D3D11_BIND_FLAG,
    ) -> Result<ID3D11Texture2D, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind_flags.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe {
            d3d11_device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("CreateTexture2D({format:?}): {e}"))?;
        }
        texture.ok_or_else(|| "CreateTexture2D returned null".to_string())
    }
}

/// Creates an `IDirect3DSurface` (WinRT) from a D3D11 texture.
/// This is used to pass our own VRAM textures into the Media Foundation encoder.
pub fn create_direct3d_surface(texture: &ID3D11Texture2D) -> Result<IDirect3DSurface, String> {
    let dxgi_surface: IDXGISurface = texture
        .cast()
        .map_err(|e| format!("Texture2D -> IDXGISurface cast failed: {e}"))?;

    let inspectable = unsafe {
        CreateDirect3D11SurfaceFromDXGISurface(&dxgi_surface)
            .map_err(|e| format!("CreateDirect3D11SurfaceFromDXGISurface failed: {e}"))?
    };

    inspectable
        .cast()
        .map_err(|e| format!("IInspectable -> IDirect3DSurface cast failed: {e}"))
}

/// D3D11 texture with an NT shared handle for cross-API interop.
///
/// The texture lives on the encode D3D11 device and is shared with wgpu (DX12)
/// via `CreateSharedHandle` / `OpenSharedHandle`. Used as a ring buffer slot
/// for zero-copy render→encode transfer.
pub struct SharedVramBuffer {
    pub texture: ID3D11Texture2D,
    pub handle: HANDLE,
}

impl SharedVramBuffer {
    /// Create a shared BGRA texture on the given D3D11 device.
    ///
    /// The texture is created with `SHARED_NTHANDLE | SHARED_KEYEDMUTEX` misc flags
    /// and `RENDER_TARGET | SHADER_RESOURCE` bind flags so it can be imported into
    /// DX12 via `OpenSharedHandle`.
    pub fn new(device: &ID3D11Device, width: u32, height: u32) -> Result<Self, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0
                | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0) as u32,
        };

        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe {
            device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("CreateTexture2D shared: {e}"))?;
        }
        let texture = texture.ok_or("CreateTexture2D shared returned null")?;

        let dxgi_resource: IDXGIResource1 = texture
            .cast()
            .map_err(|e| format!("QI IDXGIResource1: {e}"))?;

        let handle = unsafe {
            dxgi_resource
                .CreateSharedHandle(
                    None::<*const SECURITY_ATTRIBUTES>,
                    windows::Win32::Graphics::Dxgi::DXGI_SHARED_RESOURCE_READ.0
                        | windows::Win32::Graphics::Dxgi::DXGI_SHARED_RESOURCE_WRITE.0,
                    None,
                )
                .map_err(|e| format!("CreateSharedHandle: {e}"))?
        };

        Ok(Self { texture, handle })
    }
}

impl Drop for SharedVramBuffer {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

