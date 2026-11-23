//! Keeps an existing config file that could not be read from being replaced.
//!
//! Another process (antivirus, a sync client, an editor) can briefly hold
//! `config_v3.json` open. Such a read is retried; if the file stays unreadable
//! the session runs on defaults, and saves are refused so they cannot replace
//! the user's real config with those defaults.

use std::io;
use std::path::Path;
use std::time::Duration;

pub(super) const READ_ATTEMPTS: usize = 10;
pub(super) const READ_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Windows ERROR_SHARING_VIOLATION (32) and ERROR_LOCK_VIOLATION (33).
fn is_transient(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33))
}

/// Read the file, retrying while another process holds a sharing or lock
/// violation on it, up to `READ_ATTEMPTS` reads in total.
pub(super) fn read_with_retry(
    path: &Path,
    read: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    delay: Duration,
) -> io::Result<Vec<u8>> {
    let mut attempt = 1;
    loop {
        match read(path) {
            Err(error) if is_transient(&error) && attempt < READ_ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(delay);
            }
            result => return result,
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) use save_gate::unread_config_path;
#[cfg(not(feature = "recorder-worker"))]
pub(super) use save_gate::{mark_disk_config_unread, refuse_if_disk_config_unread};

#[cfg(not(feature = "recorder-worker"))]
mod save_gate {
    use std::io;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// True while the in-memory config was not loaded from an existing file.
    static DISK_CONFIG_UNREAD: AtomicBool = AtomicBool::new(false);

    pub(in crate::config::io) fn mark_disk_config_unread(unread: bool) {
        DISK_CONFIG_UNREAD.store(unread, Ordering::SeqCst);
    }

    /// The config file's path while it is unread, so the GUI can say that
    /// this session runs on defaults and will not save.
    pub(crate) fn unread_config_path() -> Option<PathBuf> {
        unread_path(&DISK_CONFIG_UNREAD, super::super::get_config_path)
    }

    pub(in crate::config::io) fn unread_path(
        flag: &AtomicBool,
        config_path: impl FnOnce() -> PathBuf,
    ) -> Option<PathBuf> {
        flag.load(Ordering::SeqCst).then(config_path)
    }

    pub(in crate::config::io) fn refuse_if_disk_config_unread() -> io::Result<()> {
        refuse_if_unread(&DISK_CONFIG_UNREAD)
    }

    pub(in crate::config::io) fn refuse_if_unread(flag: &AtomicBool) -> io::Result<()> {
        if flag.load(Ordering::SeqCst) {
            return Err(io::Error::other(
                "existing config could not be read at startup; \
                 refusing to replace it with defaults until it loads",
            ));
        }
        Ok(())
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) use save_gate::{refuse_if_unread, unread_path};
