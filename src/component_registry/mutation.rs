use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
#[cfg(not(test))]
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::core::PCWSTR;

#[cfg(not(test))]
const MUTATION_MUTEX_NAME: &str = "Global\\ScreenGoatedToolboxComponentRegistryMutation-v1";
#[cfg(not(test))]
const MUTATION_WAIT_MS: u32 = INFINITE;
#[cfg(test)]
const MUTATION_WAIT_MS: u32 = 10_000;

/// Cross-process ownership of a component-registry filesystem mutation.
///
/// Windows mutexes are recursive for their owning thread, which lets an
/// installer call the common removal path while retaining one serialization
/// boundary around the full repair transaction.
pub(crate) struct RegistryMutationGuard {
    handle: HANDLE,
}

pub(crate) fn acquire_mutation_guard() -> Result<RegistryMutationGuard> {
    #[cfg(not(test))]
    let name = MUTATION_MUTEX_NAME.to_string();
    #[cfg(test)]
    let name = format!(
        "Local\\ScreenGoatedToolboxComponentRegistryMutationTest-{}",
        std::process::id()
    );
    acquire_named(&name, MUTATION_WAIT_MS)
}

fn acquire_named(name: &str, wait_ms: u32) -> Result<RegistryMutationGuard> {
    let name = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
        .map_err(|error| anyhow!("create component-registry mutation mutex: {error}"))?;
    let wait = unsafe { WaitForSingleObject(handle, wait_ms) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(RegistryMutationGuard { handle });
    }
    unsafe {
        let _ = CloseHandle(handle);
    }
    if wait == WAIT_TIMEOUT {
        Err(anyhow!(
            "component registry is busy in another application process"
        ))
    } else {
        Err(anyhow!(
            "wait for component-registry mutation mutex failed with status {}",
            wait.0
        ))
    }
}

impl Drop for RegistryMutationGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_mutex_is_recursive_but_excludes_another_thread() {
        let name = format!(
            "Local\\ScreenGoatedToolboxComponentRegistryMutationTest-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let first = acquire_named(&name, 100).unwrap();
        let nested = acquire_named(&name, 100).unwrap();
        drop(nested);
        let contender_name = name.clone();
        let blocked = std::thread::spawn(move || acquire_named(&contender_name, 25).is_err())
            .join()
            .unwrap();
        assert!(blocked);
        drop(first);
        assert!(acquire_named(&name, 100).is_ok());
    }
}
