use super::*;
use sha2::{Digest as _, Sha256};

#[test]
fn restart_waits_for_preparation_and_aborts_cleanly_when_launch_fails() {
    use std::cell::{Cell, RefCell};
    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let events = RefCell::new(Vec::new());
    let thawed = Cell::new(false);
    let declined = guarded_restart::<_, ()>(
        || {
            events.borrow_mut().push("prepare");
            None::<Guard<'_>>
        },
        || panic!("a refused exit must not launch a helper"),
        || panic!("a refused exit must not announce restarting"),
        || events.borrow_mut().push("manual"),
        |_| panic!("a refused exit must not terminate"),
    )
    .unwrap();
    assert!(!declined);
    assert_eq!(*events.borrow(), ["prepare", "manual"]);

    let failure = guarded_restart::<_, ()>(
        || {
            events.borrow_mut().push("prepare");
            Some(Guard(&thawed))
        },
        || {
            events.borrow_mut().push("spawn");
            Err(std::io::Error::other("simulated launch failure"))
        },
        || panic!("a failed launch must not announce restarting"),
        || events.borrow_mut().push("manual"),
        |_| panic!("a failed launch must not terminate"),
    );
    assert!(failure.is_err());
    assert!(thawed.get());
    assert_eq!(
        *events.borrow(),
        ["prepare", "manual", "prepare", "spawn", "manual"]
    );

    events.borrow_mut().clear();
    let completed = guarded_restart(
        || {
            events.borrow_mut().push("prepare");
            Some(())
        },
        || {
            events.borrow_mut().push("spawn");
            Ok(())
        },
        || events.borrow_mut().push("restarting"),
        || panic!("a successful launch must not request manual restart"),
        |_| events.borrow_mut().push("exit"),
    )
    .unwrap();
    assert!(completed);
    assert_eq!(*events.borrow(), ["prepare", "spawn", "restarting", "exit"]);
}

#[test]
fn restart_reservation_survives_notification_until_exit() {
    use std::cell::Cell;
    struct Reservation<'a>(&'a Cell<bool>);
    impl Drop for Reservation<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let committed = Cell::new(false);
    let result = guarded_restart(
        || Some(()),
        || {
            committed.set(true);
            Ok(Reservation(&committed))
        },
        || assert!(committed.get()),
        || panic!("spawn succeeded"),
        |_| assert!(committed.get()),
    )
    .unwrap();
    assert!(result);
    assert!(!committed.get());
}

#[test]
fn webview_restart_has_no_one_shot_arguments_and_preserves_executable_path() {
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;
    #[cfg(windows)]
    let executable = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    #[cfg(not(windows))]
    let executable = PathBuf::from("/tmp/example");
    let command = webview2_restart_command(&executable, "123:456");
    assert_eq!(command.get_program(), executable.as_os_str());
    assert_eq!(command.get_args().count(), 0);
    assert!(command.get_envs().any(|(key, value)| {
        key == std::ffi::OsStr::new(restart::PARENT_MARKER)
            && value == Some(std::ffi::OsStr::new("123:456"))
    }));
}

fn temporary_bootstrapper(label: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sgt-webview-{label}-{}-{}.exe",
        std::process::id(),
        WEBVIEW2_DOWNLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn bounded_bootstrapper_download_rejects_size_and_hash_mismatch() {
    let mut output = Vec::new();
    assert!(download_exact(&mut &b"bytes"[..], &mut output, 4, "00").is_err());
    output.clear();
    assert!(download_exact(&mut &b"data"[..], &mut output, 4, &"0".repeat(64)).is_err());
}

#[test]
fn owned_bootstrapper_is_removed_when_locked_open_fails() {
    let path = temporary_bootstrapper("open-cleanup", b"bootstrapper");
    fs::remove_file(&path).unwrap();
    {
        let _cleanup = OwnedBootstrapperDownload::new(path.clone());
        assert!(
            with_locked_bootstrapper(&path, 12, &"0".repeat(64), |_| Ok(()), |_| Ok(())).is_err()
        );
    }
    assert!(!path.exists());
}

#[test]
fn owned_bootstrapper_is_removed_when_signature_fails() {
    let bytes = b"bootstrapper";
    let path = temporary_bootstrapper("signature-cleanup", bytes);
    let digest = format!("{:x}", Sha256::digest(bytes));
    {
        let _cleanup = OwnedBootstrapperDownload::new(path.clone());
        let result: Result<()> = with_locked_bootstrapper(
            &path,
            bytes.len() as u64,
            &digest,
            |_| bail!("simulated signature failure"),
            |_| -> Result<()> { panic!("launch must not run after signature failure") },
        );
        assert!(result.is_err());
    }
    assert!(!path.exists());
}

#[test]
fn owned_bootstrapper_is_removed_when_launch_fails() {
    let bytes = b"bootstrapper";
    let path = temporary_bootstrapper("launch-cleanup", bytes);
    let digest = format!("{:x}", Sha256::digest(bytes));
    {
        let _cleanup = OwnedBootstrapperDownload::new(path.clone());
        let result: Result<()> = with_locked_bootstrapper(
            &path,
            bytes.len() as u64,
            &digest,
            |_| Ok(()),
            |_| bail!("simulated launch failure"),
        );
        assert!(result.is_err());
    }
    assert!(!path.exists());
}

#[cfg(windows)]
#[test]
fn bootstrapper_lock_precedes_signature_and_survives_launch() {
    let bytes = b"bootstrapper";
    let path = temporary_bootstrapper("lock-lifetime", bytes);
    let digest = format!("{:x}", Sha256::digest(bytes));
    let assert_write_locked = |candidate: &Path| {
        assert!(fs::OpenOptions::new().write(true).open(candidate).is_err());
        Ok(())
    };
    with_locked_bootstrapper(
        &path,
        bytes.len() as u64,
        &digest,
        assert_write_locked,
        assert_write_locked,
    )
    .unwrap();
    fs::remove_file(path).unwrap();
}
