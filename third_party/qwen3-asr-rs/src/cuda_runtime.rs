#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;

pub fn force_cuda_requested() -> bool {
    std::env::var("SGT_QWEN3_FORCE_CUDA")
        .ok()
        .or_else(|| std::env::var("QWEN3_FORCE_CUDA").ok())
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

#[cfg(target_os = "windows")]
pub fn preload_cuda_runtime() {
    unsafe extern "system" {
        fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut core::ffi::c_void;
    }

    for dll in [
        b"c10_cuda.dll\0".as_slice(),
        b"torch_cuda.dll\0".as_slice(),
        b"cudart64_12.dll\0".as_slice(),
    ] {
        let _ = unsafe { LoadLibraryA(dll.as_ptr()) };
    }
}

#[cfg(target_os = "linux")]
pub fn preload_cuda_runtime() {
    unsafe extern "C" {
        fn dlopen(
            filename: *const core::ffi::c_char,
            flags: core::ffi::c_int,
        ) -> *mut core::ffi::c_void;
    }

    const RTLD_NOW: core::ffi::c_int = 2;
    const RTLD_GLOBAL: core::ffi::c_int = 0x100;

    for soname in [
        c"libc10_cuda.so",
        c"libtorch_cuda.so",
        c"libtorch_cuda_linalg.so",
        c"libcaffe2_nvrtc.so",
    ] {
        let _ = unsafe { dlopen(soname.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "linux")))]
pub fn preload_cuda_runtime() {}

#[cfg(target_os = "linux")]
pub fn maybe_reexec_with_cuda_preload() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;

    const MARKER_ENV: &str = "SGT_QWEN3_LINUX_CUDA_PRELOAD_DONE";

    if !force_cuda_requested() || std::env::var_os(MARKER_ENV).is_some() {
        return Ok(());
    }

    let Some(torch_lib_dir) = find_torch_lib_dir() else {
        return Ok(());
    };
    let preload_paths = cuda_preload_paths(&torch_lib_dir);
    if preload_paths.is_empty() {
        return Ok(());
    }

    let preload_value = {
        let joined = preload_paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        match std::env::var("LD_PRELOAD") {
            Ok(existing) if !existing.trim().is_empty() => format!("{joined}:{existing}"),
            _ => joined,
        }
    };

    let exe = std::env::current_exe()?;
    let err = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("LD_PRELOAD", preload_value)
        .env(MARKER_ENV, "1")
        .exec();
    Err(err)
}

#[cfg(not(target_os = "linux"))]
pub fn maybe_reexec_with_cuda_preload() -> std::io::Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn find_torch_lib_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("LIBTORCH_LIB") {
        let path = PathBuf::from(dir);
        if path.join("libtorch_cuda.so").exists() {
            return Some(path);
        }
    }

    if let Ok(dir) = std::env::var("LIBTORCH") {
        let path = PathBuf::from(dir);
        if path.join("libtorch_cuda.so").exists() {
            return Some(path);
        }
        let lib_dir = path.join("lib");
        if lib_dir.join("libtorch_cuda.so").exists() {
            return Some(lib_dir);
        }
    }

    std::env::var("LD_LIBRARY_PATH")
        .ok()
        .into_iter()
        .flat_map(|value| value.split(':').map(PathBuf::from).collect::<Vec<_>>())
        .find(|path| path.join("libtorch_cuda.so").exists())
}

#[cfg(target_os = "linux")]
fn cuda_preload_paths(torch_lib_dir: &Path) -> Vec<PathBuf> {
    [
        "libtorch_cuda.so",
        "libc10_cuda.so",
        "libtorch_cuda_linalg.so",
        "libcaffe2_nvrtc.so",
    ]
    .into_iter()
    .map(|name| torch_lib_dir.join(name))
    .filter(|path| path.exists())
    .collect()
}
