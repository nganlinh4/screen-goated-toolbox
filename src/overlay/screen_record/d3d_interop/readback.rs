use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

/// Staging texture for CPU readback of D3D11 GPU textures.
///
/// Creates a D3D11_USAGE_STAGING texture matching the source dimensions.
/// CopyResource transfers GPU->staging, then Map/Unmap reads to CPU.
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
    /// Row padding is stripped; output is tightly packed (width * 4 bytes per row).
    pub fn readback(&self, source: &ID3D11Texture2D, buf: &mut Vec<u8>) -> Result<(), String> {
        let source_res: ID3D11Resource = source
            .cast()
            .map_err(|e| format!("source->ID3D11Resource: {e}"))?;
        let staging_res: ID3D11Resource = self
            .staging
            .cast()
            .map_err(|e| format!("staging->ID3D11Resource: {e}"))?;

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

/// GPU fence using D3D11 event query; blocks CPU until all prior GPU commands complete.
///
/// Used to synchronize D3D11 VideoProcessor writes to shared textures before
/// wgpu (DX12) reads them in the render thread. The fence inserts a query
/// after the current GPU commands and spin-waits until the GPU retires them.
pub struct D3D11GpuFence {
    query: ID3D11Query,
    context: ID3D11DeviceContext,
}

impl D3D11GpuFence {
    pub fn new(device: &ID3D11Device, context: &ID3D11DeviceContext) -> Result<Self, String> {
        let desc = D3D11_QUERY_DESC {
            Query: D3D11_QUERY_EVENT,
            MiscFlags: 0,
        };
        let mut query: Option<ID3D11Query> = None;
        unsafe {
            device
                .CreateQuery(&desc, Some(&mut query))
                .map_err(|e| format!("CreateQuery(EVENT): {e}"))?;
        }
        let query = query.ok_or("CreateQuery(EVENT) returned null")?;
        Ok(Self {
            query,
            context: context.clone(),
        })
    }

    /// Insert a fence after the current GPU commands and block until they complete.
    pub fn signal_and_wait(&self) {
        unsafe {
            self.context.End(&self.query);
            self.context.Flush();
            // Poll until GPU retires all commands before the End().
            // D3D11_QUERY_EVENT data is a BOOL (i32): TRUE when complete.
            let mut spins = 0u32;
            loop {
                let mut done: i32 = 0;
                let _ = self.context.GetData(
                    &self.query,
                    Some(&mut done as *mut i32 as *mut std::ffi::c_void),
                    4,
                    0,
                );
                if done != 0 {
                    break;
                }
                spins += 1;
                if spins > 1000 {
                    std::thread::yield_now();
                }
            }
        }
    }
}
