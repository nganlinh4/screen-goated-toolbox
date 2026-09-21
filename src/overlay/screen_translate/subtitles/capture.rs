//! Persistent Windows Graphics Capture with ROI-only GPU readback.
use anyhow::{Context, Result, ensure};
use windows::{
    Graphics::{
        Capture::*,
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::IDXGIDevice,
            Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect},
        },
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
            },
        },
    },
    core::Interface,
};

pub(super) struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
    bounds: RECT,
    _apartment: Apartment,
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

impl Capture {
    pub(super) fn new(rect: RECT) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let apartment = Apartment;
            let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            ensure!(
                GetMonitorInfoW(monitor, &mut info).as_bool(),
                "monitor unavailable"
            );
            let interop: IGraphicsCaptureItemInterop =
                windows::core::factory::<GraphicsCaptureItem, _>()?;
            let item: GraphicsCaptureItem = interop.CreateForMonitor(monitor)?;
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.context("capture device missing")?;
            let context = context.context("capture context missing")?;
            let dxgi: IDXGIDevice = device.cast()?;
            let winrt: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &winrt,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                item.Size()?,
            )?;
            let session = pool.CreateCaptureSession(&item)?;
            let _ = session.SetIsCursorCaptureEnabled(false);
            // Optional on older Windows; the system capture indicator is not SGT's glow.
            let _ = session.SetIsBorderRequired(false);
            session.StartCapture()?;
            Ok(Self {
                pool,
                session,
                device,
                context,
                staging: None,
                size: (0, 0),
                bounds: info.rcMonitor,
                _apartment: apartment,
            })
        }
    }

    pub(super) fn frame(&mut self, rect: RECT) -> Result<Option<image::RgbaImage>> {
        unsafe {
            self.device.GetDeviceRemovedReason()?;
        }
        let mut latest = None;
        for _ in 0..3 {
            match self.pool.TryGetNextFrame() {
                Ok(frame) => {
                    if let Some(old) = latest.replace(frame) {
                        let _: windows::core::Result<()> = old.Close();
                    }
                }
                // WinRT returns success + a null interface when the queue is empty.
                // The non-null Rust projection exposes that nullable result as Err(S_OK).
                Err(error)
                    if error.code().is_ok()
                        || error.code() == windows::Win32::Foundation::E_POINTER =>
                {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        let Some(frame) = latest else {
            return Ok(None);
        };
        let result = self.read(&frame, rect);
        let _ = frame.Close();
        result.map(Some)
    }

    fn read(&mut self, frame: &Direct3D11CaptureFrame, rect: RECT) -> Result<image::RgbaImage> {
        let x = rect.left - self.bounds.left;
        let y = rect.top - self.bounds.top;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let content = frame.ContentSize()?;
        ensure!(
            x >= 0
                && y >= 0
                && width > 0
                && height > 0
                && x + width <= content.Width
                && y + height <= content.Height,
            "subtitle region is outside the captured monitor"
        );
        let size = (width as u32, height as u32);
        unsafe {
            let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
            let source: ID3D11Texture2D = access.GetInterface()?;
            if self.size != size {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source.GetDesc(&mut desc);
                desc.Width = size.0;
                desc.Height = size.1;
                desc.MipLevels = 1;
                desc.ArraySize = 1;
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.MiscFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut self.staging))?;
                self.size = size;
            }
            let target = self
                .staging
                .as_ref()
                .context("capture staging texture missing")?;
            self.context.CopySubresourceRegion(
                target,
                0,
                0,
                0,
                0,
                &source,
                0,
                Some(&D3D11_BOX {
                    left: x as u32,
                    top: y as u32,
                    right: (x + width) as u32,
                    bottom: (y + height) as u32,
                    front: 0,
                    back: 1,
                }),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(target, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut image = image::RgbaImage::new(size.0, size.1);
            for row in 0..size.1 as usize {
                let input = std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    size.0 as usize * 4,
                );
                let output =
                    &mut image.as_mut()[row * size.0 as usize * 4..(row + 1) * size.0 as usize * 4];
                for (source, target) in input
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(output.as_chunks_mut::<4>().0.iter_mut())
                {
                    target.copy_from_slice(&[source[2], source[1], source[0], 255]);
                }
            }
            self.context.Unmap(target, 0);
            Ok(image)
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
