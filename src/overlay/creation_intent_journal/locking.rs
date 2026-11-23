//! Cross-process serialization for the creation intent journal.

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows::core::PCWSTR;

    const NAME: &str = "Global\\ScreenGoatedToolboxCreationIntentJournal-v1";
    const WAIT_MS: u32 = 10_000;

    pub(crate) struct Guard(HANDLE);

    pub(crate) fn acquire() -> Result<Guard, String> {
        acquire_named(NAME, WAIT_MS)
    }

    fn acquire_named(name: &str, timeout: u32) -> Result<Guard, String> {
        let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
        let result = unsafe { WaitForSingleObject(handle, timeout) };
        if result == WAIT_OBJECT_0 || result == WAIT_ABANDONED {
            return Ok(Guard(handle));
        }
        unsafe {
            let _ = CloseHandle(handle);
        }
        Err("Creation recovery state is busy or unavailable.".to_string())
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

        #[test]
        fn named_mutex_serializes_and_recovers_abandoned_ownership() {
            let name = format!(
                "Local\\SgtIntentJournalTest-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let first = acquire_named(&name, 100).unwrap();
            let other_name = name.clone();
            assert!(
                std::thread::spawn(move || acquire_named(&other_name, 25).is_err())
                    .join()
                    .unwrap()
            );
            drop(first);
            let abandoned_name = name.clone();
            let handle = std::thread::spawn(move || {
                let guard =
                    std::mem::ManuallyDrop::new(acquire_named(&abandoned_name, 100).unwrap());
                guard.0.0 as isize
            })
            .join()
            .unwrap();
            let recovered = acquire_named(&name, 100).unwrap();
            drop(recovered);
            unsafe {
                let _ = CloseHandle(HANDLE(handle as *mut std::ffi::c_void));
            }
        }

        #[test]
        fn independent_processes_serialize_read_modify_write() {
            const CHILD: &str = "SGT_JOURNAL_LOCK_TEST_CHILD";
            const NAME: &str = "SGT_JOURNAL_LOCK_TEST_NAME";
            const PATH: &str = "SGT_JOURNAL_LOCK_TEST_PATH";
            if std::env::var_os(CHILD).is_some() {
                let name = std::env::var(NAME).unwrap();
                let path = std::env::var_os(PATH).unwrap();
                for _ in 0..12 {
                    let _guard = acquire_named(&name, 10_000).unwrap();
                    let value: u32 = std::fs::read_to_string(&path).unwrap().parse().unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    std::fs::write(&path, (value + 1).to_string()).unwrap();
                }
                return;
            }
            let root = std::env::temp_dir().join(format!(
                "sgt-intent-lock-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir(&root).unwrap();
            let path = root.join("count");
            std::fs::write(&path, "0").unwrap();
            let name = format!(
                "Local\\SgtIntentProcessTest-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let mut children = (0..2).map(|_| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg("overlay::creation_intent_journal::locking::platform::tests::independent_processes_serialize_read_modify_write")
                    .env(CHILD, "1").env(NAME, &name).env(PATH, &path)
                    .spawn().unwrap()
            }).collect::<Vec<_>>();
            for child in &mut children {
                assert!(child.wait().unwrap().success());
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "24");
            std::fs::remove_file(path).unwrap();
            std::fs::remove_dir(root).unwrap();
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::sync::{LazyLock, Mutex, MutexGuard};
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    pub(crate) struct Guard {
        _guard: MutexGuard<'static, ()>,
    }
    pub(crate) fn acquire() -> Result<Guard, String> {
        LOCK.lock()
            .map(|guard| Guard { _guard: guard })
            .map_err(|_| "Creation recovery state is unavailable.".to_string())
    }
}

pub(super) use platform::acquire;
