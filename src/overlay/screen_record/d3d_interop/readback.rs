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
    ///
    /// Returns `Err` if the device is lost (e.g. `DXGI_ERROR_DEVICE_REMOVED` on a TDR
    /// / driver reset) instead of spinning forever on a query that will never signal.
    /// After a short spin budget it backs off with a tiny sleep so a slow frame doesn't
    /// pin a CPU core and starve the driver threads.
    pub fn signal_and_wait(&self) -> Result<(), String> {
        self.signal_and_wait_inner(None)
    }

    pub fn signal_and_wait_cancellable(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        self.signal_and_wait_inner(Some(cancel))
    }

    fn signal_and_wait_inner(
        &self,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("GPU operation cancelled before D3D11 fence".to_string());
        }
        unsafe {
            self.context.End(&self.query);
            self.context.Flush();
            wait_for_query(cancel, std::time::Duration::from_secs(30), || {
                let mut done: i32 = 0;
                let hr = self.context.GetData(
                    &self.query,
                    Some(&mut done as *mut i32 as *mut std::ffi::c_void),
                    4,
                    0,
                );
                if hr.is_err() {
                    return Err(format!(
                        "D3D11 GetData failed waiting for GPU fence (device removed / TDR?): {hr:?}"
                    ));
                }
                Ok(done != 0)
            })
        }
    }
}

fn wait_for_query(
    cancel: Option<&std::sync::atomic::AtomicBool>,
    timeout: std::time::Duration,
    mut poll: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    let mut spins = 0u32;
    loop {
        if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("GPU operation cancelled while waiting for D3D11 completion".to_string());
        }
        if poll()? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err("Timed out waiting for D3D11 GPU completion".to_string());
        }
        spins = spins.saturating_add(1);
        if spins > 1000 {
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    }
}

#[cfg(test)]
mod fence_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn query_wait_reports_timeout_cancel_and_driver_failure() {
        let cancel = AtomicBool::new(false);
        assert!(
            wait_for_query(Some(&cancel), std::time::Duration::ZERO, || Ok(false))
                .unwrap_err()
                .contains("Timed out")
        );
        assert!(
            wait_for_query(Some(&cancel), std::time::Duration::from_secs(1), || Err(
                "device failed".to_string()
            ))
            .unwrap_err()
            .contains("device failed")
        );
        cancel.store(true, Ordering::Release);
        assert!(
            wait_for_query(Some(&cancel), std::time::Duration::from_secs(1), || panic!(
                "must not poll after cancellation"
            ))
            .unwrap_err()
            .contains("cancel")
        );
        cancel.store(false, Ordering::Release);
        assert!(
            wait_for_query(Some(&cancel), std::time::Duration::from_secs(1), || Ok(
                true
            ))
            .is_ok()
        );
    }
}
