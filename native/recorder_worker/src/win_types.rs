use windows::Win32::Foundation::HWND;

#[derive(Clone, Copy, Debug, Default)]
pub struct SendHwnd(pub HWND);
unsafe impl Send for SendHwnd {}
unsafe impl Sync for SendHwnd {}

impl SendHwnd {
    pub fn is_invalid(&self) -> bool {
        self.0.is_invalid()
    }

    pub fn as_isize(&self) -> isize {
        self.0.0 as isize
    }
}

pub struct HwndWrapper(pub HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let hwnd = self.0.0 as isize;
        let non_zero =
            std::num::NonZeroIsize::new(hwnd).ok_or(raw_window_handle::HandleError::Unavailable)?;
        let mut handle = raw_window_handle::Win32WindowHandle::new(non_zero);
        handle.hinstance = None;
        let raw = raw_window_handle::RawWindowHandle::Win32(handle);
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}
