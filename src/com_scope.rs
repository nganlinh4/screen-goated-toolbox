//! Scoped COM apartment ownership for the calling thread.

use std::marker::PhantomData;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT, CoInitializeEx, CoUninitialize};

/// Holds one COM initialization lease until dropped, so a borrowed thread
/// leaves the scope in the apartment state it entered with. A thread already
/// in a different apartment keeps it; the scope then acquires and releases
/// nothing. Declare it before any COM object so it drops after them.
pub(crate) struct ComScope {
    acquired: bool,
    // COM leases are per-thread; the scope must be dropped where it was made.
    _thread: PhantomData<*const ()>,
}

impl ComScope {
    pub(crate) fn enter(model: COINIT) -> windows::core::Result<Self> {
        let status = unsafe { CoInitializeEx(None, model) };
        // S_OK and S_FALSE both take a lease that needs a matching uninitialize.
        let acquired = if status.is_ok() {
            true
        } else if status == RPC_E_CHANGED_MODE {
            false
        } else {
            return Err(windows::core::Error::from_hresult(status));
        };
        Ok(Self {
            acquired,
            _thread: PhantomData,
        })
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.acquired {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};

    #[test]
    fn scope_releases_its_apartment_and_tolerates_a_foreign_one() {
        std::thread::spawn(|| {
            drop(ComScope::enter(COINIT_APARTMENTTHREADED).unwrap());
            // The STA lease is gone, so the thread may now join the MTA.
            let mta = ComScope::enter(COINIT_MULTITHREADED).unwrap();
            assert!(mta.acquired);
            let nested = ComScope::enter(COINIT_APARTMENTTHREADED).unwrap();
            assert!(!nested.acquired, "a foreign apartment must not be released");
        })
        .join()
        .unwrap();
    }
}
