use alloc::sync::Arc;
use core::{ffi, ptr};

use once_cell::sync::Lazy;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{Direct2D::Common::D2D_RECT_F, DirectComposition},
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOREDIRECTIONBITMAP,
        },
    },
    core::Interface as _,
};

use super::DynLib;

// Lazy-loaded DirectComposition library
#[derive(Debug)]
pub(crate) struct DCompLib {
    lib: Lazy<Result<DynLib, crate::SurfaceError>>,
}

impl DCompLib {
    pub(crate) fn new() -> Self {
        Self {
            lib: Lazy::new(|| unsafe {
                DynLib::new("dcomp.dll").map_err(|err| {
                    log::error!("Error loading dcomp.dll: {err}");
                    crate::SurfaceError::Other("Error loading dcomp.dll")
                })
            }),
        }
    }

    fn get_lib(&self) -> Result<&DynLib, crate::SurfaceError> {
        match self.lib.as_ref() {
            Ok(lib) => Ok(lib),
            Err(err) => Err(err.clone()),
        }
    }

    pub(crate) fn create_device(
        &self,
    ) -> Result<DirectComposition::IDCompositionDevice, crate::SurfaceError> {
        let lib = self.get_lib()?;

        // Calls windows::Win32::Graphics::DirectComposition::DCompositionCreateDevice2 on dcomp.dll
        type Fun = extern "system" fn(
            pdxdevice: *mut ffi::c_void,
            riid: *const windows_core::GUID,
            ppdcompdevice: *mut *mut ffi::c_void,
        ) -> windows_core::HRESULT;
        let func: libloading::Symbol<Fun> =
            unsafe { lib.get(c"DCompositionCreateDevice2".to_bytes()) }?;

        let mut res: Option<DirectComposition::IDCompositionDevice> = None;

        (func)(
            ptr::null_mut(),
            &DirectComposition::IDCompositionDevice::IID,
            <*mut _>::cast(&mut res),
        )
        .map(|| res.unwrap())
        .map_err(|err| {
            log::error!("DirectComposition::DCompositionCreateDevice2 failed: {err}");
            crate::SurfaceError::Other("DirectComposition::DCompositionCreateDevice2")
        })
    }
}

#[derive(Default)]
pub struct DCompState {
    inner: Option<InnerState>,
}

impl DCompState {
    /// This will create a DirectComposition device and a target for the window handle if not already initialized.
    /// If the device is already initialized, it will return the existing state.
    pub unsafe fn get_or_init(
        &mut self,
        lib: &Arc<DCompLib>,
        hwnd: &HWND,
    ) -> Result<&mut InnerState, crate::SurfaceError> {
        if self.inner.is_none() {
            self.inner = Some(unsafe { InnerState::init(lib, hwnd) }?);
        }
        Ok(self.inner.as_mut().unwrap())
    }

    pub fn set_clip(&mut self, width: u32, height: u32) -> Result<bool, crate::SurfaceError> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(false);
        };
        let clip = D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: width as f32,
            bottom: height as f32,
        };
        unsafe { inner.visual.SetClip2(&raw const clip) }.map_err(|err| {
            log::error!("IDCompositionVisual::SetClip failed: {err}");
            crate::SurfaceError::Other("IDCompositionVisual::SetClip")
        })?;
        unsafe { inner.device.Commit() }.map_err(|err| {
            log::error!("IDCompositionDevice::Commit failed: {err}");
            crate::SurfaceError::Other("IDCompositionDevice::Commit")
        })?;
        Ok(true)
    }
}

pub struct InnerState {
    pub visual: DirectComposition::IDCompositionVisual,
    pub device: DirectComposition::IDCompositionDevice,
    // Must be kept alive but is otherwise unused after initialization.
    pub _target: DirectComposition::IDCompositionTarget,
}

impl InnerState {
    /// Creates a DirectComposition device and a target for the given window handle.
    pub unsafe fn init(lib: &Arc<DCompLib>, hwnd: &HWND) -> Result<Self, crate::SurfaceError> {
        profiling::scope!("DCompState::init");

        // This HWND is presented exclusively through DirectComposition. Disable DWM's separate
        // redirected backing bitmap before binding the visual tree so window geometry and visual
        // content use one composition path during interactive resizing.
        let ex_style = unsafe { GetWindowLongPtrW(*hwnd, GWL_EXSTYLE) };
        if ex_style & WS_EX_NOREDIRECTIONBITMAP.0 as isize == 0 {
            unsafe {
                SetWindowLongPtrW(
                    *hwnd,
                    GWL_EXSTYLE,
                    ex_style | WS_EX_NOREDIRECTIONBITMAP.0 as isize,
                )
            };
        }

        let dcomp_device = lib.create_device()?;

        let target = unsafe { dcomp_device.CreateTargetForHwnd(*hwnd, false) }.map_err(|err| {
            log::error!("IDCompositionDevice::CreateTargetForHwnd failed: {err}");
            crate::SurfaceError::Other("IDCompositionDevice::CreateTargetForHwnd")
        })?;

        let visual = unsafe { dcomp_device.CreateVisual() }.map_err(|err| {
            log::error!("IDCompositionDevice::CreateVisual failed: {err}");
            crate::SurfaceError::Other("IDCompositionDevice::CreateVisual")
        })?;

        unsafe { target.SetRoot(&visual) }.map_err(|err| {
            log::error!("IDCompositionTarget::SetRoot failed: {err}");
            crate::SurfaceError::Other("IDCompositionTarget::SetRoot")
        })?;

        Ok(InnerState {
            visual,
            device: dcomp_device,
            _target: target,
        })
    }
}
