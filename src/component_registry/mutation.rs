use anyhow::{Result, anyhow};
use std::sync::atomic::{AtomicBool, Ordering};
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
    acquire_named(&mutex_name(), MUTATION_WAIT_MS)
}

pub(crate) fn acquire_mutation_guard_cancellable(
    cancelled: &AtomicBool,
) -> Result<RegistryMutationGuard> {
    acquire_named_cancellable(&mutex_name(), cancelled)
}

/// Cross-process ownership of one component's transfer. Downloads run under
/// this per-component guard instead of the registry mutation guard, so a long
/// transfer never blocks other components' installs, removals or opens, while
/// two processes still never download the same component at once.
pub(crate) fn acquire_transfer_guard_cancellable(
    component: &str,
    cancelled: &AtomicBool,
) -> Result<RegistryMutationGuard> {
    super::catalog::validate_identifier(component)?;
    acquire_named_cancellable(&format!("{}-transfer-{component}", mutex_name()), cancelled)
}

fn mutex_name() -> String {
    #[cfg(not(test))]
    let name = MUTATION_MUTEX_NAME.to_string();
    #[cfg(test)]
    let name = format!(
        "Local\\ScreenGoatedToolboxComponentRegistryMutationTest-{}",
        std::process::id()
    );
    name
}

fn create_named(name: &str) -> Result<HANDLE> {
    let name = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
        .map_err(|error| anyhow!("create component-registry mutation mutex: {error}"))
}

fn acquire_named_cancellable(name: &str, cancelled: &AtomicBool) -> Result<RegistryMutationGuard> {
    let handle = create_named(name)?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(anyhow!(
                "component preparation cancelled while waiting for registry"
            ));
        }
        let wait = unsafe { WaitForSingleObject(handle, 20) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        let guard = finish_wait(handle, wait)?;
        if cancelled.load(Ordering::Acquire) {
            drop(guard);
            return Err(anyhow!(
                "component preparation cancelled after acquiring registry"
            ));
        }
        return Ok(guard);
    }
}

fn acquire_named(name: &str, wait_ms: u32) -> Result<RegistryMutationGuard> {
    let handle = create_named(name)?;
    let wait = unsafe { WaitForSingleObject(handle, wait_ms) };
    finish_wait(handle, wait)
}

fn finish_wait(
    handle: HANDLE,
    wait: windows::Win32::Foundation::WAIT_EVENT,
) -> Result<RegistryMutationGuard> {
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

    fn test_name() -> String {
        format!(
            "Local\\ScreenGoatedToolboxComponentRegistryMutationTest-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )
    }

    #[test]
    fn cancellation_interrupts_a_wait_without_releasing_another_threads_mutex() {
        let name = test_name();
        let first = acquire_named(&name, 100).unwrap();
        let cancelled = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let (started, ready) = std::sync::mpsc::channel();
            let (done, result) = std::sync::mpsc::channel();
            let name = &name;
            let cancelled = &cancelled;
            scope.spawn(move || {
                started.send(()).unwrap();
                done.send(acquire_named_cancellable(name, cancelled).is_err())
                    .unwrap();
            });
            ready.recv().unwrap();
            cancelled.store(true, Ordering::Release);
            assert!(
                result
                    .recv_timeout(std::time::Duration::from_secs(1))
                    .unwrap()
            );
            assert!(
                scope
                    .spawn(move || acquire_named(name, 25).is_err())
                    .join()
                    .unwrap()
            );
        });
        drop(first);
        assert!(acquire_named(&name, 100).is_ok());
    }

    #[test]
    fn a_cancelled_acquisition_does_not_take_a_free_mutex() {
        let name = test_name();
        assert!(acquire_named_cancellable(&name, &AtomicBool::new(true)).is_err());
        assert!(acquire_named(&name, 100).is_ok());
    }

    #[test]
    fn a_cancellable_acquisition_remains_recursive_on_its_owning_thread() {
        let name = test_name();
        let first = acquire_named(&name, 100).unwrap();
        let nested = acquire_named_cancellable(&name, &AtomicBool::new(false)).unwrap();
        drop(nested);
        assert!(std::thread::scope(|scope| scope
            .spawn(|| acquire_named(&name, 25).is_err())
            .join()
            .unwrap()));
        drop(first);
        assert!(acquire_named(&name, 100).is_ok());
    }

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
