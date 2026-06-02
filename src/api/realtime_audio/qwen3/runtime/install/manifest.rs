use super::super::{
    QWEN3_LIBTORCH_REQUIRED_DLLS, QWEN3_RUNTIME_ABI_CACHE, QWEN3_RUNTIME_ABI_VERSION,
    QWEN3_RUNTIME_DLL, QWEN3_RUNTIME_MANIFEST, QWEN3_RUNTIME_REQUIRED_DLLS, RUNTIME_MANIFEST_URL,
    RuntimeAbiProbe, RuntimeDownloadManifest, RuntimeVersionFn, clear_runtime_notice,
};
use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) fn qwen3_runtime_dir_is_usable(dir: &Path) -> bool {
    qwen3_runtime_required_files_present(dir)
        && qwen3_runtime_abi_version(dir.join(QWEN3_RUNTIME_DLL).as_path())
            == Some(QWEN3_RUNTIME_ABI_VERSION)
        && if dir == crate::unpack_dlls::private_bin_dir().as_path() {
            qwen3_runtime_managed_manifest_is_usable(dir)
        } else {
            true
        }
}

fn qwen3_runtime_required_files_present(dir: &Path) -> bool {
    QWEN3_RUNTIME_REQUIRED_DLLS
        .iter()
        .all(|name| dir.join(name).is_file())
}

pub(super) fn qwen3_libtorch_required_files_present(dir: &Path) -> bool {
    QWEN3_LIBTORCH_REQUIRED_DLLS
        .iter()
        .all(|name| dir.join(name).is_file())
}

pub(super) fn runtime_manifest_path(dir: &Path) -> PathBuf {
    dir.join(QWEN3_RUNTIME_MANIFEST)
}

fn qwen3_runtime_managed_manifest_is_usable(dir: &Path) -> bool {
    let manifest_path = runtime_manifest_path(dir);
    let runtime_dll_path = dir.join(QWEN3_RUNTIME_DLL);
    read_runtime_download_manifest(&manifest_path)
        .and_then(|manifest| verify_runtime_dll_against_manifest(&runtime_dll_path, &manifest))
        .is_ok()
}

fn read_runtime_download_manifest(path: &Path) -> Result<RuntimeDownloadManifest> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read runtime manifest '{}'", path.display()))?;
    let manifest: RuntimeDownloadManifest = serde_json::from_str(&body)
        .with_context(|| format!("Failed to parse runtime manifest '{}'", path.display()))?;
    if manifest.abi_version != QWEN3_RUNTIME_ABI_VERSION {
        bail!(
            "Runtime manifest ABI mismatch: expected {}, got {}",
            QWEN3_RUNTIME_ABI_VERSION,
            manifest.abi_version
        );
    }
    if manifest.sha256.trim().is_empty() {
        bail!("Runtime manifest sha256 was empty");
    }
    Ok(manifest)
}

pub(super) fn fetch_runtime_download_manifest() -> Result<RuntimeDownloadManifest> {
    let response = ureq::get(RUNTIME_MANIFEST_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch Qwen3 runtime manifest: {err}"))?;
    let mut reader = response.into_body().into_reader();
    let mut body = String::new();
    reader.read_to_string(&mut body)?;
    let manifest: RuntimeDownloadManifest = serde_json::from_str(&body)
        .map_err(|err| anyhow!("Failed to parse Qwen3 runtime manifest: {err}"))?;
    if manifest.abi_version != QWEN3_RUNTIME_ABI_VERSION {
        bail!(
            "Qwen3 runtime manifest ABI mismatch: expected {}, got {}",
            QWEN3_RUNTIME_ABI_VERSION,
            manifest.abi_version
        );
    }
    if manifest.sha256.trim().is_empty() {
        bail!("Qwen3 runtime manifest sha256 was empty");
    }
    Ok(manifest)
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open '{}' for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn verify_runtime_dll_against_manifest(
    runtime_dll_path: &Path,
    manifest: &RuntimeDownloadManifest,
) -> Result<()> {
    verify_runtime_dll_file_against_manifest(runtime_dll_path, manifest)?;
    let actual_abi = qwen3_runtime_abi_version(runtime_dll_path).unwrap_or_default();
    if actual_abi != manifest.abi_version {
        bail!(
            "Runtime DLL ABI mismatch: expected {}, got {}",
            manifest.abi_version,
            actual_abi
        );
    }
    Ok(())
}

pub(super) fn verify_runtime_dll_file_against_manifest(
    runtime_dll_path: &Path,
    manifest: &RuntimeDownloadManifest,
) -> Result<()> {
    let metadata = std::fs::metadata(runtime_dll_path)
        .with_context(|| format!("Failed to inspect '{}'", runtime_dll_path.display()))?;
    if metadata.len() != manifest.size {
        bail!(
            "Runtime DLL size mismatch: expected {} bytes, got {} bytes",
            manifest.size,
            metadata.len()
        );
    }
    let actual_sha256 = sha256_hex(runtime_dll_path)?;
    if actual_sha256 != manifest.sha256.to_ascii_lowercase() {
        bail!(
            "Runtime DLL checksum mismatch: expected {}, got {}",
            manifest.sha256,
            actual_sha256
        );
    }
    Ok(())
}

fn qwen3_runtime_abi_version(runtime_dll_path: &Path) -> Option<u32> {
    let metadata = std::fs::metadata(runtime_dll_path).ok()?;
    let probe = RuntimeAbiProbe {
        size: metadata.len(),
        modified: metadata.modified().ok(),
        abi_version: None,
    };

    if let Ok(cache) = QWEN3_RUNTIME_ABI_CACHE.lock()
        && let Some(cached) = cache.get(runtime_dll_path)
        && cached.size == probe.size
        && cached.modified == probe.modified
    {
        return cached.abi_version;
    }

    let abi_version = unsafe {
        #[cfg(target_os = "windows")]
        let _dll_dir_guard = runtime_dll_path.parent().map(TemporaryDllDirectory::new);
        let library = Library::new(runtime_dll_path).ok()?;
        let version = *library
            .get::<RuntimeVersionFn>(b"sgt_qwen3_runtime_version\0")
            .ok()?;
        Some(version())
    };

    if let Ok(mut cache) = QWEN3_RUNTIME_ABI_CACHE.lock() {
        cache.insert(
            runtime_dll_path.to_path_buf(),
            RuntimeAbiProbe {
                abi_version,
                ..probe
            },
        );
    }
    abi_version
}

#[cfg(target_os = "windows")]
struct TemporaryDllDirectory {
    previous: Vec<u16>,
}

#[cfg(target_os = "windows")]
impl TemporaryDllDirectory {
    fn new(dir: &Path) -> Self {
        use windows::Win32::System::LibraryLoader::{GetDllDirectoryW, SetDllDirectoryW};
        let mut previous = vec![0u16; 32_768];
        let previous_len = unsafe { GetDllDirectoryW(Some(&mut previous)) } as usize;
        previous.truncate(previous_len);
        let dir_wide: Vec<u16> = dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
        }
        Self { previous }
    }
}

#[cfg(target_os = "windows")]
impl Drop for TemporaryDllDirectory {
    fn drop(&mut self) {
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
        let mut previous = self.previous.clone();
        previous.push(0);
        unsafe {
            let _ = SetDllDirectoryW(windows::core::PCWSTR(previous.as_ptr()));
        }
    }
}

/// Check if the runtime is installed in the managed (downloadable) private bin dir.
/// Used by settings UI — doesn't count dev build paths.
pub fn is_qwen3_runtime_managed_installed() -> bool {
    qwen3_runtime_dir_is_usable(&crate::unpack_dlls::private_bin_dir())
}

pub fn qwen3_runtime_installed_size() -> u64 {
    let bin_dir = crate::unpack_dlls::private_bin_dir();
    if !qwen3_runtime_dir_is_usable(&bin_dir) {
        return 0;
    }
    // Count only libtorch + runtime DLLs, not other SGT tools in the same dir
    std::fs::read_dir(&bin_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name == QWEN3_RUNTIME_DLL
                        || name.starts_with("torch")
                        || name.starts_with("c10")
                        || name.starts_with("cuda")
                        || name.starts_with("cublas")
                        || name.starts_with("cudnn")
                        || name.starts_with("nvrtc")
                        || name.starts_with("nvJitLink")
                        || name.starts_with("caffe2")
                        || name.starts_with("fbgemm")
                        || name.starts_with("asmjit")
                        || name.starts_with("gomp")
                        || name.starts_with("uv")
                })
                .filter_map(|e| e.metadata().ok().map(|m| m.len()))
                .sum()
        })
        .unwrap_or(0)
}

pub fn remove_qwen3_runtime() -> anyhow::Result<()> {
    let bin_dir = crate::unpack_dlls::private_bin_dir();
    // Remove only the runtime DLL and libtorch DLLs, not other SGT support DLLs
    let runtime_dll_names: &[&str] = &[
        "sgt_qwen3_runtime.dll",
        "torch_cpu.dll",
        "torch_cuda.dll",
        "torch.dll",
        "c10.dll",
        "c10_cuda.dll",
    ];
    for name in runtime_dll_names {
        let _ = std::fs::remove_file(bin_dir.join(name));
    }
    let _ = std::fs::remove_file(runtime_manifest_path(&bin_dir));
    // Remove all libtorch-related DLLs (cuda*, cudnn*, nvrtc*, caffe2*, etc.)
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("cuda")
                || name.starts_with("cublas")
                || name.starts_with("cudnn")
                || name.starts_with("nvrtc")
                || name.starts_with("nvJitLink")
                || name.starts_with("caffe2")
                || name.starts_with("gomp")
                || name.starts_with("fbgemm")
                || name.starts_with("asmjit")
                || name.starts_with("uv")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let _ = std::fs::remove_file(bin_dir.join("libtorch-download.zip"));
    if let Ok(mut cache) = QWEN3_RUNTIME_ABI_CACHE.lock() {
        cache.retain(|path, _| !path.starts_with(&bin_dir));
    }
    clear_runtime_notice();
    Ok(())
}
