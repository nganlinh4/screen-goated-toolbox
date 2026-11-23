//! Cross-process serialization of the result history store.

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows::core::PCWSTR;

    const NAME: &str = "Global\\ScreenGoatedToolboxCreationHistory-v1";
    const WAIT_MS: u32 = 10_000;

    pub(crate) struct Guard(HANDLE);

    pub(crate) fn acquire() -> Result<Guard, String> {
        acquire_named(&name(), WAIT_MS)?.ok_or_else(|| "Result history is busy.".to_string())
    }

    pub(crate) fn try_acquire() -> Result<Option<Guard>, String> {
        acquire_named(&name(), 0)
    }

    fn name() -> String {
        #[cfg(test)]
        if let Ok(name) = std::env::var("SGT_TEST_HISTORY_MUTEX_NAME") {
            return name;
        }
        NAME.to_string()
    }

    fn acquire_named(name: &str, timeout: u32) -> Result<Option<Guard>, String> {
        let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|_| "Result history is unavailable.".to_string())?;
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
            Err("Result history is unavailable.".to_string())
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::overlay::generation_history::{load_store, record_at};

        #[test]
        fn abandoned_mutex_is_recovered_and_try_lock_is_nonblocking() {
            let name = format!(
                "Local\\SgtHistoryLock-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let guard = acquire_named(&name, 100).unwrap().unwrap();
            let other = name.clone();
            assert!(
                std::thread::spawn(move || acquire_named(&other, 0).unwrap().is_none())
                    .join()
                    .unwrap()
            );
            drop(guard);
            let other = name.clone();
            let leaked = std::thread::spawn(move || {
                let guard =
                    std::mem::ManuallyDrop::new(acquire_named(&other, 100).unwrap().unwrap());
                guard.0.0 as isize
            })
            .join()
            .unwrap();
            let recovered = acquire_named(&name, 100).unwrap().unwrap();
            drop(recovered);
            unsafe {
                let _ = CloseHandle(HANDLE(leaked as *mut std::ffi::c_void));
            }
        }

        #[test]
        fn independent_processes_retain_all_recorded_history() {
            const CHILD: &str = "SGT_TEST_HISTORY_PROCESS_CHILD";
            const ROOT: &str = "SGT_TEST_HISTORY_PROCESS_ROOT";
            if let Ok(worker) = std::env::var(CHILD) {
                let root = std::path::PathBuf::from(std::env::var_os(ROOT).unwrap());
                let index_path = root.join("history.json");
                for index in 0..8 {
                    let artifact = root.join(format!("result-{worker}-{index}.png"));
                    std::fs::write(&artifact, format!("content-{worker}-{index}")).unwrap();
                    let _guard = acquire().unwrap();
                    record_at(
                        &index_path,
                        "image",
                        "",
                        artifact.to_str().unwrap(),
                        serde_json::Value::Null,
                        200,
                        (None, None),
                    )
                    .unwrap();
                }
                return;
            }
            let root = std::env::temp_dir().join(format!(
                "sgt-history-lock-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir(&root).unwrap();
            let name = format!(
                "Local\\SgtHistoryProcess-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let mut children = ["a", "b"].into_iter().map(|worker| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg("overlay::generation_history::locking::platform::tests::independent_processes_retain_all_recorded_history")
                    .env(CHILD, worker).env(ROOT, &root).env("SGT_TEST_HISTORY_MUTEX_NAME", &name)
                    .env("SGT_RUNTIME_STATE_ROOT", &root)
                    .spawn().unwrap()
            }).collect::<Vec<_>>();
            for child in &mut children {
                assert!(child.wait().unwrap().success());
            }
            let store = load_store(&root.join("history.json")).unwrap();
            assert_eq!(store.entries.len(), 16);
            let unique = store
                .entries
                .iter()
                .map(|entry| &entry.id)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(unique.len(), 16);
            for entry in &store.entries {
                std::fs::remove_file(&entry.output_path).unwrap();
            }
            std::fs::remove_file(root.join("history.json")).unwrap();
            std::fs::remove_dir(root).unwrap();
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::sync::{LazyLock, Mutex, MutexGuard, TryLockError};
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    pub(crate) struct Guard {
        _guard: MutexGuard<'static, ()>,
    }
    pub(crate) fn acquire() -> Result<Guard, String> {
        LOCK.lock()
            .map(|guard| Guard { _guard: guard })
            .map_err(|_| "Result history is unavailable.".to_string())
    }
    pub(crate) fn try_acquire() -> Result<Option<Guard>, String> {
        match LOCK.try_lock() {
            Ok(guard) => Ok(Some(Guard { _guard: guard })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(_)) => Err("Result history is unavailable.".to_string()),
        }
    }
}

pub(super) use platform::{acquire, try_acquire};
