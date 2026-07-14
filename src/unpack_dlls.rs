mod ai_runtime;
mod remote_zip;

use std::fs;
use std::path::{Path, PathBuf};
use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

pub use self::ai_runtime::{
    AiRuntimeStatus, AiRuntimeUi, ai_runtime_version_label, current_ai_runtime_notice,
    current_ai_runtime_status, ensure_ai_runtime_installed, is_ai_runtime_installed,
    remove_ai_runtime, start_ai_runtime_install,
};

pub(crate) fn private_bin_dir() -> PathBuf {
    let arch_dir = match crate::runtime_support::current_process_arch() {
        crate::runtime_support::RuntimeArch::Arm64 => "arm64",
        crate::runtime_support::RuntimeArch::X64 => "x64",
    };

    crate::paths::app_local_data_dir()
        .join("bin")
        .join(arch_dir)
}

fn ensure_private_bin_dir_exists(bin_dir: &Path) {
    let _ = fs::create_dir_all(bin_dir);
}

fn bundled_support_dlls() -> &'static [(&'static str, &'static [u8])] {
    match crate::runtime_support::current_process_arch() {
        crate::runtime_support::RuntimeArch::Arm64 => &[
            (
                "concrt140.dll",
                include_bytes!("embed_dlls/arm64/concrt140.dll"),
            ),
            (
                "msvcp140.dll",
                include_bytes!("embed_dlls/arm64/msvcp140.dll"),
            ),
            (
                "msvcp140_1.dll",
                include_bytes!("embed_dlls/arm64/msvcp140_1.dll"),
            ),
            (
                "msvcp140_2.dll",
                include_bytes!("embed_dlls/arm64/msvcp140_2.dll"),
            ),
            (
                "msvcp140_atomic_wait.dll",
                include_bytes!("embed_dlls/arm64/msvcp140_atomic_wait.dll"),
            ),
            (
                "msvcp140_codecvt_ids.dll",
                include_bytes!("embed_dlls/arm64/msvcp140_codecvt_ids.dll"),
            ),
            (
                "vccorlib140.dll",
                include_bytes!("embed_dlls/arm64/vccorlib140.dll"),
            ),
            (
                "vcruntime140.dll",
                include_bytes!("embed_dlls/arm64/vcruntime140.dll"),
            ),
            (
                "vcruntime140_1.dll",
                include_bytes!("embed_dlls/arm64/vcruntime140_1.dll"),
            ),
            (
                "vcruntime140_threads.dll",
                include_bytes!("embed_dlls/arm64/vcruntime140_threads.dll"),
            ),
        ],
        crate::runtime_support::RuntimeArch::X64 => &[
            (
                "concrt140.dll",
                include_bytes!("embed_dlls/x64/concrt140.dll"),
            ),
            (
                "msvcp140.dll",
                include_bytes!("embed_dlls/x64/msvcp140.dll"),
            ),
            (
                "msvcp140_1.dll",
                include_bytes!("embed_dlls/x64/msvcp140_1.dll"),
            ),
            (
                "msvcp140_2.dll",
                include_bytes!("embed_dlls/x64/msvcp140_2.dll"),
            ),
            (
                "msvcp140_atomic_wait.dll",
                include_bytes!("embed_dlls/x64/msvcp140_atomic_wait.dll"),
            ),
            (
                "msvcp140_codecvt_ids.dll",
                include_bytes!("embed_dlls/x64/msvcp140_codecvt_ids.dll"),
            ),
            (
                "vccorlib140.dll",
                include_bytes!("embed_dlls/x64/vccorlib140.dll"),
            ),
            (
                "vcruntime140.dll",
                include_bytes!("embed_dlls/x64/vcruntime140.dll"),
            ),
            (
                "vcruntime140_1.dll",
                include_bytes!("embed_dlls/x64/vcruntime140_1.dll"),
            ),
            (
                "vcruntime140_threads.dll",
                include_bytes!("embed_dlls/x64/vcruntime140_threads.dll"),
            ),
        ],
    }
}

fn unpack_support_dlls(bin_dir: &Path) {
    for (name, bytes) in bundled_support_dlls() {
        let path = bin_dir.join(name);
        let _ = fs::write(&path, bytes);
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

/// Prepare the private runtime directory and unpack the app-local VC CRT for the current
/// architecture. The large local AI runtime is installed on demand via
/// `ensure_ai_runtime_installed`.
pub fn unpack_dlls() {
    let bin_dir = private_bin_dir();
    ensure_private_bin_dir_exists(&bin_dir);
    unpack_support_dlls(&bin_dir);
    configure_private_bin_dir(&bin_dir);

    crate::log_info!("[Unpacker] Support DLLs verified/unpacked to {:?}", bin_dir);
}
