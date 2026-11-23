//! Cross-process serialization for cooperating exact file writers.

use super::{Path, TransactionFailure};
use crate::crypto::sha256_hex;
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::core::PCWSTR;

const MUTEX_WAIT_MS: u32 = 30_000;

pub(super) struct NamedMutexGuard {
    handle: HANDLE,
}

impl NamedMutexGuard {
    pub(super) fn acquire(path: &Path) -> Result<Self, TransactionFailure> {
        let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
        let suffix = sha256_hex(normalized.as_bytes());
        let name: Vec<u16> = format!("Local\\SGT.CC.TextEdit.{suffix}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|error| TransactionFailure::Io(error.to_string()))?;
        let wait = unsafe { WaitForSingleObject(handle, MUTEX_WAIT_MS) };
        if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
            Ok(Self { handle })
        } else {
            unsafe {
                let _ = CloseHandle(handle);
            }
            if wait == WAIT_TIMEOUT {
                Err(TransactionFailure::Busy(
                    "another exact edit still owns this path".to_string(),
                ))
            } else {
                Err(TransactionFailure::Io(format!(
                    "waiting for the path edit mutex failed with status {}",
                    wait.0
                )))
            }
        }
    }
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}
