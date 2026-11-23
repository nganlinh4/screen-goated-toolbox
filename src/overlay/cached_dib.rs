//! A reusable top-down 32-bit DIB section for layered overlay rendering.
//!
//! The handle is cleared the moment it is deleted, so a failed re-create can
//! never leave a stale handle behind for a later call to delete again.

use std::sync::Mutex;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, GetDC,
    HBITMAP, ReleaseDC,
};

pub(crate) struct CachedDib {
    bitmap: HBITMAP,
    bits: *mut u8,
    width: i32,
    height: i32,
}

// SAFETY: a DIB section handle is a process-wide GDI object and `bits` points
// into memory owned by that object. Every access goes through the owning
// Mutex, so the handle and its pixels are never used from two threads at once.
unsafe impl Send for CachedDib {}

impl CachedDib {
    pub(crate) const fn empty() -> Self {
        Self {
            bitmap: HBITMAP(std::ptr::null_mut()),
            bits: std::ptr::null_mut(),
            width: 0,
            height: 0,
        }
    }

    /// Ensures a `width` x `height` surface. Returns `Some(true)` when it was
    /// (re)created, `Some(false)` when the cached one was reused, and `None`
    /// when no surface is available.
    pub(crate) fn ensure(&mut self, width: i32, height: i32) -> Option<bool> {
        if !self.bitmap.is_invalid() && self.width == width && self.height == height {
            return Some(false);
        }
        self.release();
        if width <= 0 || height <= 0 {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let created = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
            let _ = ReleaseDC(None, screen);
            let bitmap = created.ok().filter(|bitmap| !bitmap.is_invalid())?;
            if bits.is_null() {
                let _ = DeleteObject(bitmap.into());
                return None;
            }
            self.bitmap = bitmap;
            self.bits = bits.cast();
        }
        self.width = width;
        self.height = height;
        Some(true)
    }

    pub(crate) fn bitmap(&self) -> HBITMAP {
        self.bitmap
    }

    /// The surface pixels, `width * height` BGRA values; null when empty.
    pub(crate) fn bits(&self) -> *mut u8 {
        self.bits
    }

    /// Deletes the surface and forgets it in the same step.
    pub(crate) fn release(&mut self) {
        let bitmap = std::mem::replace(&mut self.bitmap, HBITMAP(std::ptr::null_mut()));
        self.bits = std::ptr::null_mut();
        self.width = 0;
        self.height = 0;
        if !bitmap.is_invalid() {
            unsafe {
                let _ = DeleteObject(bitmap.into());
            }
        }
    }
}

impl Drop for CachedDib {
    fn drop(&mut self) {
        self.release();
    }
}

pub(crate) fn with<T>(dib: &Mutex<CachedDib>, effect: impl FnOnce(&mut CachedDib) -> T) -> T {
    effect(&mut dib.lock().unwrap_or_else(|error| error.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::CachedDib;

    #[test]
    fn failed_recreate_forgets_the_deleted_handle() {
        let mut dib = CachedDib::empty();
        assert_eq!(dib.ensure(8, 8), Some(true));
        assert!(!dib.bitmap().is_invalid());
        assert!(!dib.bits().is_null());
        assert_eq!(dib.ensure(8, 8), Some(false));

        // A size that cannot be created must not keep the old, deleted handle.
        assert_eq!(dib.ensure(0, 8), None);
        assert!(dib.bitmap().is_invalid());
        assert!(dib.bits().is_null());
        dib.release();

        assert_eq!(dib.ensure(4, 4), Some(true));
        dib.release();
        dib.release();
        assert!(dib.bitmap().is_invalid());
    }

    #[test]
    fn repeated_resizes_do_not_leak_gdi_objects() {
        let mut dib = CachedDib::empty();
        let before = crate::screen_capture::gui_resources_snapshot().gdi_objects;
        for size in 1..=64 {
            assert_eq!(dib.ensure(size, size), Some(true));
            assert_eq!(dib.ensure(0, 0), None);
        }
        dib.release();
        let after = crate::screen_capture::gui_resources_snapshot().gdi_objects;
        assert!(
            after < before + 8,
            "GDI objects grew from {before} to {after}"
        );
    }
}
