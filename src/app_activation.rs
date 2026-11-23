//! Activation of the single running desktop instance.
//!
//! Later launches hand requests to the owner over its same-user,
//! integrity-restricted pipe (see `app_entry::single_instance`). The owner
//! admits them here, in memory: no other process can open the restore signal,
//! the admission lock or the pending-file list, and nothing is read from TEMP.

pub(crate) mod admission;
mod ready_gate;

#[cfg(test)]
#[path = "app_activation/admission_tests.rs"]
mod admission_tests;

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::core::PCWSTR;

use crate::win_types::SendHandle;

/// Unnamed auto-reset event waited on only by the owner's listener. Keeping
/// exactly one waiter makes one wakeup responsible for files and restore.
pub static RESTORE_EVENT: LazyLock<Option<SendHandle>> = LazyLock::new(|| unsafe {
    CreateEventW(None, false, false, PCWSTR::null())
        .ok()
        .map(SendHandle)
});

static PENDING_FILES: PendingFiles = PendingFiles::new();

/// Files admitted by the owner and not yet dispatched.
struct PendingFiles(Mutex<VecDeque<PathBuf>>);

impl PendingFiles {
    const fn new() -> Self {
        Self(Mutex::new(VecDeque::new()))
    }

    fn files(&self) -> std::sync::MutexGuard<'_, VecDeque<PathBuf>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn push(&self, path: PathBuf) {
        self.files().push_back(path);
    }

    fn is_empty(&self) -> bool {
        self.files().is_empty()
    }

    /// Callers hold the activation admission lock.
    fn drain(&self, mut dispatch: impl FnMut(PathBuf) -> bool) -> bool {
        let pending = std::mem::take(&mut *self.files());
        let mut processed = false;
        for path in pending {
            processed |= dispatch(path);
        }
        processed
    }
}

pub(crate) fn single_instance_mutex_name_wide() -> Vec<u16> {
    namespaced_object_name("ScreenGoatedToolboxSingleInstanceMutex")
}

fn namespaced_object_name(base: &str) -> Vec<u16> {
    format!("Global\\{base}-{}", current_exe_namespace_suffix())
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

fn current_exe_namespace_suffix() -> String {
    let path = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .map(|path| {
            path.to_string_lossy()
                .replace('/', "\\")
                .to_ascii_lowercase()
        })
        .unwrap_or_else(|| "unknown".to_string());
    let state = crate::paths::runtime_state_root().map(|root| {
        std::path::absolute(&root)
            .unwrap_or(root)
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
    });
    namespace_suffix(&path, state.as_deref())
}

fn namespace_suffix(path: &str, state_root: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    if let Some(root) = state_root {
        hasher.update([0]);
        hasher.update(root.as_bytes());
    }
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(32);
    for byte in digest.iter().take(16) {
        suffix.push_str(&format!("{byte:02x}"));
    }
    suffix
}

/// Admit a request received by the owner: queue its file, then wake the listener.
pub(crate) fn admit_activation(pending_file: Option<&Path>) -> io::Result<()> {
    let queued = pending_file
        .map(|path| {
            let _admission = admission::acquire()?;
            PENDING_FILES.push(path.to_owned());
            Ok(())
        })
        .transpose();

    let signalled = match RESTORE_EVENT.as_ref() {
        Some(event) => {
            unsafe { SetEvent(event.0) }.map_err(|error| io::Error::other(error.to_string()))
        }
        None => Err(io::Error::other("restore event unavailable")),
    };
    activation_delivery_result(queued.map(|_| ()), signalled)
}

fn activation_delivery_result(queued: io::Result<()>, signalled: io::Result<()>) -> io::Result<()> {
    queued.and(signalled)
}

static APP_READY: ready_gate::ReadyGate = ready_gate::ReadyGate::new();

/// Lets the listener deliver queued files and restores. Called once the
/// startup work that file handling relies on has finished.
pub(crate) fn mark_app_ready() {
    APP_READY.open();
}

fn deliver_when_ready(gate: &ready_gate::ReadyGate, deliver: impl FnOnce() -> bool) -> bool {
    gate.wait();
    deliver()
}

/// Start the sole wait loop in the process that owns the singleton mutex.
pub(crate) fn start_listener() {
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::spawn(wait_for_activations);
    });
}

fn wait_for_activations() {
    let Some(event) = RESTORE_EVENT.as_ref() else {
        crate::log_info!("[Activation] Restore event unavailable; polling pending files");
        APP_READY.wait();
        poll_pending_files();
        return;
    };

    deliver_when_ready(&APP_READY, process_pending_files);
    loop {
        let result = unsafe { WaitForSingleObject(event.0, 5_000) };
        if result != WAIT_OBJECT_0 && result != WAIT_TIMEOUT {
            crate::log_info!("[Activation] Restore event wait ended with {result:?}");
            poll_pending_files();
            return;
        }

        process_activation_wakeup(result == WAIT_OBJECT_0, process_pending_files, || {
            crate::gui::app::accept_restore_activation();
        });
    }
}

fn poll_pending_files() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        process_activation_wakeup(false, process_pending_files, || {
            crate::gui::app::accept_restore_activation();
        });
    }
}

fn process_activation_wakeup(
    signalled: bool,
    process: impl FnOnce() -> bool,
    restore: impl FnOnce(),
) {
    let processed = process();
    if signalled || processed {
        restore();
    }
}

fn process_pending_files() -> bool {
    process_pending_files_with(|path| {
        std::thread::spawn(move || {
            crate::gui::app::input_handler::process_file_path(&path);
        });
    })
}

fn process_pending_files_with(mut dispatch: impl FnMut(PathBuf)) -> bool {
    let Ok(_admission) = admission::acquire() else {
        return false;
    };
    PENDING_FILES.drain(|path| {
        crate::gui::app::mark_secondary_file_activation();
        if !path.exists() {
            return false;
        }
        crate::log_info!("[Activation] Processing pending file: {:?}", path);
        dispatch(path);
        true
    })
}

pub(crate) fn reserve_restart_admission() -> io::Result<admission::AdmissionGuard> {
    let guard = admission::acquire()?;
    if crate::gui::app::automatic_restart_blocked_by_file() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "file activation already accepted",
        ));
    }
    if !PENDING_FILES.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "file activation is queued",
        ));
    }
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::{namespace_suffix, namespaced_object_name};

    #[test]
    fn isolated_roots_have_distinct_activation_namespaces() {
        let executable = r"c:\app\sgt.exe";
        let production = namespace_suffix(executable, None);
        let first = namespace_suffix(executable, Some(r"c:\isolated-a"));
        let second = namespace_suffix(executable, Some(r"c:\isolated-b"));
        assert_ne!(production, first);
        assert_ne!(first, second);
        assert_eq!(first, namespace_suffix(executable, Some(r"c:\isolated-a")));
    }

    #[test]
    fn kernel_object_names_are_global_namespaced_and_null_terminated() {
        let name = namespaced_object_name("ActivationContract");
        let decoded = String::from_utf16(&name[..name.len() - 1]).unwrap();

        assert!(decoded.starts_with("Global\\ActivationContract-"));
        assert_eq!(name.last(), Some(&0));
        assert_eq!(decoded.rsplit('-').next().map(str::len), Some(32));
    }

    #[test]
    fn failed_signal_or_queue_is_not_reported_as_delivery() {
        let failed = || std::io::Error::other("unavailable");
        assert!(super::activation_delivery_result(Ok(()), Err(failed())).is_err());
        assert!(super::activation_delivery_result(Err(failed()), Ok(())).is_err());
        assert!(super::activation_delivery_result(Ok(()), Ok(())).is_ok());
    }
}
