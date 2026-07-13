//! Cross-process activation for the single running desktop instance.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Once};

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForSingleObject};
use windows::core::PCWSTR;

use crate::win_types::SendHandle;

/// Auto-reset event used only by the singleton owner. Keeping exactly one waiter
/// makes one activation responsible for both file delivery and window restore.
pub static RESTORE_EVENT: LazyLock<Option<SendHandle>> = LazyLock::new(|| unsafe {
    let name = restore_event_name_wide();
    CreateEventW(None, false, false, PCWSTR(name.as_ptr()))
        .ok()
        .map(SendHandle)
});

pub(crate) fn restore_event_name_wide() -> Vec<u16> {
    namespaced_object_name("ScreenGoatedToolboxRestoreEvent")
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
    let digest = Sha256::digest(path.as_bytes());
    let mut suffix = String::with_capacity(32);
    for byte in digest.iter().take(16) {
        suffix.push_str(&format!("{byte:02x}"));
    }
    suffix
}

pub(crate) fn notify_primary_instance(pending_file: Option<&Path>) {
    if let Some(path) = pending_file
        && let Err(error) = enqueue_pending_file(path)
    {
        crate::log_info!("[Activation] Failed to queue pending file: {error}");
    }

    if let Some(event) = RESTORE_EVENT.as_ref() {
        unsafe {
            let _ = SetEvent(event.0);
        }
    }
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
        crate::log_info!("[Activation] Restore event unavailable; listener not started");
        return;
    };

    loop {
        let result = unsafe { WaitForSingleObject(event.0, INFINITE) };
        if result != WAIT_OBJECT_0 {
            crate::log_info!("[Activation] Restore event wait ended with {result:?}");
            return;
        }

        process_pending_files();
        crate::gui::app::accept_restore_activation();
    }
}

fn enqueue_pending_file(path: &Path) -> io::Result<()> {
    let queue_dir = activation_queue_dir();
    std::fs::create_dir_all(&queue_dir)?;

    static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stem = pending_file_stem(
        timestamp,
        std::process::id(),
        NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed),
    );
    let temporary = queue_dir.join(format!("{stem}.tmp"));
    let ready = queue_dir.join(format!("{stem}.pending"));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(path.to_string_lossy().as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, ready)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn process_pending_files() {
    let Ok(entries) = std::fs::read_dir(activation_queue_dir()) else {
        return;
    };
    let mut pending = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pending")
        })
        .collect::<Vec<_>>();
    pending.sort();

    for queued_file in pending {
        let Ok(content) = std::fs::read_to_string(&queued_file) else {
            continue;
        };
        let _ = std::fs::remove_file(&queued_file);
        let path = std::path::PathBuf::from(content.trim());
        if path.exists() {
            crate::log_info!("[Activation] Processing pending file: {:?}", path);
            std::thread::spawn(move || {
                crate::gui::app::input_handler::process_file_path(&path);
            });
        }
    }
}

fn activation_queue_dir() -> PathBuf {
    std::env::temp_dir().join(format!("sgt-activation-{}", current_exe_namespace_suffix()))
}

fn pending_file_stem(timestamp: u128, process_id: u32, sequence: u64) -> String {
    format!("{timestamp:039}-{process_id:010}-{sequence:020}")
}

#[cfg(test)]
mod tests {
    use super::{activation_queue_dir, namespaced_object_name, pending_file_stem};

    #[test]
    fn kernel_object_names_are_global_namespaced_and_null_terminated() {
        let name = namespaced_object_name("ActivationContract");
        let decoded = String::from_utf16(&name[..name.len() - 1]).unwrap();

        assert!(decoded.starts_with("Global\\ActivationContract-"));
        assert_eq!(name.last(), Some(&0));
        assert_eq!(decoded.rsplit('-').next().map(str::len), Some(32));
    }

    #[test]
    fn pending_file_queue_is_namespaced_and_uniquely_ordered() {
        let queue_name = activation_queue_dir()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let first = pending_file_stem(12, 34, 0);
        let second = pending_file_stem(12, 34, 1);

        assert!(queue_name.starts_with("sgt-activation-"));
        assert_eq!(queue_name.len(), "sgt-activation-".len() + 32);
        assert!(first < second);
        assert_ne!(first, pending_file_stem(12, 35, 0));
    }
}
