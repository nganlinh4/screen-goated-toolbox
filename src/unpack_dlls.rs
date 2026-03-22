mod ai_runtime;
mod remote_zip;

use std::fs;
use std::path::{Path, PathBuf};
use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

pub use self::ai_runtime::{
    AiRuntimeStatus, AiRuntimeUi, ai_runtime_version_label, current_ai_runtime_notice,
    current_ai_runtime_status, ensure_ai_runtime_installed, remove_ai_runtime,
    start_ai_runtime_install,
};

pub(super) fn private_bin_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
}

fn ensure_private_bin_dir_exists(bin_dir: &Path) {
    let _ = fs::create_dir_all(bin_dir);
}

fn unpack_support_dlls(bin_dir: &Path) {
    let dlls: &[(&str, &[u8])] = &[
        (
            "vcruntime140.dll",
            include_bytes!("embed_dlls/vcruntime140.dll"),
        ),
        (
            "vcruntime140_1.dll",
            include_bytes!("embed_dlls/vcruntime140_1.dll"),
        ),
        ("msvcp140.dll", include_bytes!("embed_dlls/msvcp140.dll")),
        (
            "msvcp140_1.dll",
            include_bytes!("embed_dlls/msvcp140_1.dll"),
        ),
    ];

    for (name, bytes) in dlls {
        let path = bin_dir.join(name);
        if !path.exists() {
            let _ = fs::write(&path, bytes);
        }
    }
}

fn configure_private_bin_dir(bin_dir: &Path) {
    unsafe {
        let path_wide: Vec<u16> = bin_dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = SetDllDirectoryW(windows::core::PCWSTR(path_wide.as_ptr()));
    }

    if let Ok(current_path) = std::env::var("PATH") {
        let new_path = format!("{};{}", bin_dir.to_string_lossy(), current_path);
        unsafe {
            std::env::set_var("PATH", new_path);
        }
    }
}

/// Prepare the private runtime directory and unpack only the small CRT support DLLs.
/// The large local AI runtime is installed on demand via `ensure_ai_runtime_installed`.
pub fn unpack_dlls() {
    let bin_dir = private_bin_dir();
    ensure_private_bin_dir_exists(&bin_dir);
    unpack_support_dlls(&bin_dir);
    configure_private_bin_dir(&bin_dir);

    crate::log_info!("[Unpacker] Support DLLs verified/unpacked to {:?}", bin_dir);
}
