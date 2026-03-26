use windows::Win32::Foundation::{GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12 as d3d12;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

use super::super::d3d_interop::SharedVramBuffer;
use super::types::{DecodeInputRing, GpuOutputRing, DECODE_RING_SIZE, GPU_RING_SIZE};

/// Import a shared D3D11 texture (NT handle) into wgpu as a DX12 texture.
///
/// Bridges windows 0.62 (our crate) <-> windows 0.58 (wgpu-hal) by reinterpreting
/// COM pointers. Both versions are ABI-identical `#[repr(transparent)]` wrappers.
pub(super) unsafe fn import_shared_handle_into_wgpu(
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

    // Open the shared NT handle -> D3D12 resource (windows 0.62).
    let mut d3d12_resource: Option<d3d12::ID3D12Resource> = None;
    unsafe {
        our_d12
            .OpenSharedHandle(handle, &mut d3d12_resource)
            .map_err(|e| format!("OpenSharedHandle: {e}"))?;
    }
    let d3d12_resource =
        d3d12_resource.ok_or_else(|| "OpenSharedHandle returned null".to_string())?;

    // Convert 0.62 ID3D12Resource -> 0.58 for texture_from_raw.
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
/// Returns None if any step fails -- caller should fall back to CPU path.
pub(super) fn try_create_gpu_output_ring(
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
        // COPY_SRC: state-reset read after each write -- forces wgpu to insert a
        //   COPY_SRC -> COPY_DST barrier (with cache flush) on the next frame.
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

/// Try to create a decode input ring (shared VRAM textures for decode->render).
///
/// Uses D3D11-created `SHARED_KEYEDMUTEX | SHARED_NTHANDLE` textures with cross-API
/// shared fence. wgpu imports with `COPY_SRC | COPY_DST` so the render thread can
/// force a COPY_DST->COPY_SRC barrier (L2 cache flush) each frame via a 1-pixel
/// `copy_buffer_to_texture` before the full `copy_texture_to_texture`.
pub(super) fn try_create_decode_input_ring(
    dec_device: &ID3D11Device,
    wgpu_device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Option<DecodeInputRing> {
    if std::env::var("SGT_FORCE_CPU_DECODE").is_ok() {
        eprintln!("[Export] SGT_FORCE_CPU_DECODE: forcing CPU decode path");
        return None;
    }

    // -- Create cross-API shared fence --

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

    eprintln!("[Export] Cross-API shared fence created (D3D12->D3D11)");

    // -- Create shared texture ring --
    // SHARED_NTHANDLE requires SHARED_KEYEDMUTEX (D3D11 API constraint).
    // Keyed mutex provides CPU-level ownership. The shared fence provides
    // GPU ordering. A per-frame COPY_DST->COPY_SRC barrier forces L2 flush.

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
        //   COPY_DST->COPY_SRC barrier (with L2 cache flush) on the next copy.
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
