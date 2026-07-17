mod ai_runtime;
mod remote_zip;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

pub(crate) use self::ai_runtime::ensure_onnx_runtime_initialized;
pub use self::ai_runtime::{
    AiRuntimeStatus, AiRuntimeUi, ai_runtime_version_label, current_ai_runtime_notice,
    current_ai_runtime_status, ensure_ai_runtime_installed, is_ai_runtime_installed,
    remove_ai_runtime, start_ai_runtime_install,
};

fn arch_dir() -> &'static str {
    match crate::runtime_support::current_process_arch() {
        crate::runtime_support::RuntimeArch::Arm64 => "arm64",
        crate::runtime_support::RuntimeArch::X64 => "x64",
    }
}

pub(crate) fn private_bin_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("bin")
        .join(arch_dir())
}

fn runtime_support_bin_dir() -> PathBuf {
    crate::paths::app_runtime_local_data_dir()
        .join("bin")
        .join(arch_dir())
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

fn configure_dll_search(support_bin_dir: &Path, ai_runtime_bin_dir: &Path) {
    unsafe {
        let path_wide: Vec<u16> = support_bin_dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = SetDllDirectoryW(windows::core::PCWSTR(path_wide.as_ptr()));
    }

    if let Ok(new_path) = compose_dll_search_path(
        [support_bin_dir, ai_runtime_bin_dir],
        std::env::var_os("PATH").as_deref(),
    ) {
        unsafe {
            std::env::set_var("PATH", new_path);
        }
    }
}

fn compose_dll_search_path<'a>(
    preferred: impl IntoIterator<Item = &'a Path>,
    current: Option<&OsStr>,
) -> Result<OsString, std::env::JoinPathsError> {
    let mut entries: Vec<PathBuf> = Vec::new();
    let mut push_unique = |path: &Path| {
        let duplicate = entries.iter().any(|existing| {
            existing
                .to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        });
        if !duplicate {
            entries.push(path.to_path_buf());
        }
    };
    for path in preferred {
        push_unique(path);
    }
    if let Some(current) = current {
        for path in std::env::split_paths(current) {
            push_unique(&path);
        }
    }
    std::env::join_paths(entries)
}

/// Prepare the private runtime directory and unpack the app-local VC CRT for the current
/// architecture. The large local AI runtime is installed on demand via
/// `ensure_ai_runtime_installed`.
pub fn unpack_dlls() {
    let support_bin_dir = runtime_support_bin_dir();
    ensure_private_bin_dir_exists(&support_bin_dir);
    unpack_support_dlls(&support_bin_dir);
    configure_dll_search(&support_bin_dir, &private_bin_dir());

    crate::log_info!(
        "[Unpacker] Support DLLs verified/unpacked to {:?}",
        support_bin_dir
    );
}

#[cfg(test)]
mod tests {
    use super::compose_dll_search_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn dll_search_keeps_support_and_installed_runtime_ahead_of_existing_path() {
        let existing =
            std::env::join_paths([Path::new(r"C:\Windows\System32"), Path::new(r"C:\Tools")])
                .expect("compose fixture path");
        let composed = compose_dll_search_path(
            [Path::new(r"C:\Run\support"), Path::new(r"C:\App\bin")],
            Some(&existing),
        )
        .expect("compose DLL search path");
        let entries: Vec<PathBuf> = std::env::split_paths(&composed).collect();

        assert_eq!(
            entries,
            vec![
                PathBuf::from(r"C:\Run\support"),
                PathBuf::from(r"C:\App\bin"),
                PathBuf::from(r"C:\Windows\System32"),
                PathBuf::from(r"C:\Tools"),
            ]
        );
    }

    #[test]
    fn dll_search_deduplicates_existing_entries_case_insensitively() {
        let existing = std::env::join_paths([Path::new(r"c:\app\BIN"), Path::new(r"C:\Tools")])
            .expect("compose fixture path");
        let composed = compose_dll_search_path(
            [Path::new(r"C:\Run\support"), Path::new(r"C:\App\bin")],
            Some(&existing),
        )
        .expect("compose DLL search path");
        let entries: Vec<PathBuf> = std::env::split_paths(&composed).collect();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1], PathBuf::from(r"C:\App\bin"));
    }
}
