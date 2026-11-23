//! Ownership of endpoint identifiers allocated by IMMDevice::GetId.

use windows::Win32::Media::Audio::IMMDevice;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::PWSTR;

struct OwnedId<F: FnOnce(PWSTR)> {
    pointer: PWSTR,
    free: Option<F>,
}

impl<F: FnOnce(PWSTR)> Drop for OwnedId<F> {
    fn drop(&mut self) {
        if let Some(free) = self.free.take() {
            free(self.pointer);
        }
    }
}

/// The caller must hold a valid COM apartment for `device`.
pub(crate) unsafe fn read(device: &IMMDevice) -> windows::core::Result<Option<String>> {
    let pointer = unsafe { device.GetId()? };
    Ok(unsafe { copy_owned(pointer) })
}

unsafe fn copy_owned(pointer: PWSTR) -> Option<String> {
    unsafe { copy_owned_with(pointer, |pointer| CoTaskMemFree(Some(pointer.0.cast()))) }
}

unsafe fn copy_owned_with(pointer: PWSTR, free: impl FnOnce(PWSTR)) -> Option<String> {
    let owned = OwnedId {
        pointer,
        free: Some(free),
    };
    if owned.pointer.is_null() {
        None
    } else {
        unsafe { owned.pointer.to_string().ok() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::CoTaskMemAlloc;

    unsafe fn allocated(units: &[u16]) -> PWSTR {
        let pointer = unsafe { CoTaskMemAlloc(std::mem::size_of_val(units)) }.cast::<u16>();
        assert!(!pointer.is_null());
        unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len()) };
        PWSTR(pointer)
    }

    #[test]
    fn copies_valid_and_rejects_invalid_utf16_without_retaining_com_storage() {
        unsafe {
            let frees = std::cell::Cell::new(0);
            let copy = |units: &[u16]| {
                copy_owned_with(allocated(units), |pointer| {
                    frees.set(frees.get() + 1);
                    CoTaskMemFree(Some(pointer.0.cast()));
                })
            };
            assert_eq!(copy(&[65, 0xd83d, 0xde00, 0]), Some("A😀".into()));
            assert_eq!(copy(&[0xd800, 0]), None);
            assert_eq!(frees.get(), 2);
            assert_eq!(copy_owned(PWSTR::null()), None);
        }
    }
}
