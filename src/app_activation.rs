//! Cross-process activation for the single running desktop instance.

use std::io::Write;
use std::path::Path;
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
        && let Ok(mut file) = std::fs::File::create(pending_file_path())
    {
        let _ = write!(file, "{}", path.to_string_lossy());
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

        process_pending_file();
        crate::gui::app::accept_restore_activation();
    }
}

fn process_pending_file() {
    let temp_file = pending_file_path();
    if !temp_file.exists() {
        return;
    }

    if let Ok(content) = std::fs::read_to_string(&temp_file) {
        let path = std::path::PathBuf::from(content.trim());
        if path.exists() {
            crate::log_info!("[Activation] Processing pending file: {:?}", path);
            std::thread::spawn(move || {
                crate::gui::app::input_handler::process_file_path(&path);
            });
        }
    }
    let _ = std::fs::remove_file(temp_file);
}

fn pending_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("sgt_pending_file.txt")
}

#[cfg(test)]
mod tests {
    use super::namespaced_object_name;

    #[test]
    fn kernel_object_names_are_global_namespaced_and_null_terminated() {
        let name = namespaced_object_name("ActivationContract");
        let decoded = String::from_utf16(&name[..name.len() - 1]).unwrap();

        assert!(decoded.starts_with("Global\\ActivationContract-"));
        assert_eq!(name.last(), Some(&0));
        assert_eq!(decoded.rsplit('-').next().map(str::len), Some(32));
    }
}
