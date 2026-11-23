//! Cross-process ownership of Creation source and staging lifetimes.

mod admission;
pub(crate) use admission::{AdmissionGate, count_pending, register_admission};

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows::core::PCWSTR;

    const WAIT_MS: u32 = 10_000;

    pub(crate) struct Guard(HANDLE);

    pub(crate) fn acquire() -> Result<Guard, String> {
        acquire_named("Global\\ScreenGoatedToolboxCreationLifecycle-v1")
    }

    pub(crate) fn acquire_snapshot() -> Result<Guard, String> {
        acquire_named("Global\\ScreenGoatedToolboxCreationSnapshot-v1")
    }

    pub(crate) fn acquire_presentation() -> Result<Guard, String> {
        acquire_named("Global\\ScreenGoatedToolboxCreationPresentation-v1")
    }

    pub(crate) fn acquire_delivery() -> Result<Guard, String> {
        acquire_named("Global\\ScreenGoatedToolboxCreationDelivery-v1")
    }

    pub(crate) fn try_acquire_delivery() -> Result<Option<Guard>, String> {
        try_named("Global\\ScreenGoatedToolboxCreationDelivery-v1", 0)
    }

    fn acquire_named(name: &str) -> Result<Guard, String> {
        try_named(name, WAIT_MS)?
            .ok_or_else(|| "Creation storage is busy or unavailable.".to_string())
    }

    fn try_named(name: &str, timeout: u32) -> Result<Option<Guard>, String> {
        let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|_| "Creation storage is unavailable.".to_string())?;
        let result = unsafe { WaitForSingleObject(handle, timeout) };
        if result == WAIT_OBJECT_0 || result == WAIT_ABANDONED {
            return Ok(Some(Guard(handle)));
        }
        unsafe {
            let _ = CloseHandle(handle);
        }
        if result == WAIT_TIMEOUT {
            Ok(None)
        } else {
            Err("Creation storage is unavailable.".to_string())
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseMutex(self.0);
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::sync::LazyLock;
    static LOCK: LazyLock<parking_lot::ReentrantMutex<()>> =
        LazyLock::new(|| parking_lot::ReentrantMutex::new(()));
    static SNAPSHOT: LazyLock<parking_lot::ReentrantMutex<()>> =
        LazyLock::new(|| parking_lot::ReentrantMutex::new(()));
    static PRESENTATION: LazyLock<parking_lot::ReentrantMutex<()>> =
        LazyLock::new(|| parking_lot::ReentrantMutex::new(()));
    static DELIVERY: LazyLock<parking_lot::ReentrantMutex<()>> =
        LazyLock::new(|| parking_lot::ReentrantMutex::new(()));
    pub(crate) struct Guard {
        _guard: parking_lot::ReentrantMutexGuard<'static, ()>,
    }
    pub(crate) fn acquire() -> Result<Guard, String> {
        Ok(Guard {
            _guard: LOCK.lock(),
        })
    }
    pub(crate) fn acquire_snapshot() -> Result<Guard, String> {
        Ok(Guard {
            _guard: SNAPSHOT.lock(),
        })
    }
    pub(crate) fn acquire_presentation() -> Result<Guard, String> {
        Ok(Guard {
            _guard: PRESENTATION.lock(),
        })
    }
    pub(crate) fn acquire_delivery() -> Result<Guard, String> {
        Ok(Guard {
            _guard: DELIVERY.lock(),
        })
    }
    pub(crate) fn try_acquire_delivery() -> Result<Option<Guard>, String> {
        Ok(DELIVERY.try_lock().map(|guard| Guard { _guard: guard }))
    }
}

pub(crate) use platform::{
    acquire, acquire_delivery, acquire_presentation, acquire_snapshot, try_acquire_delivery,
};
