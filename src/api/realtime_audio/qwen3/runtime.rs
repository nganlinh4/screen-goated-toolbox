use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::SystemTime;

lazy_static::lazy_static! {
    static ref LAST_QWEN3_RUNTIME_NOTICE: Mutex<Option<String>> = Mutex::new(None);
    static ref QWEN3_RUNTIME_ABI_CACHE: Mutex<HashMap<PathBuf, RuntimeAbiProbe>> =
        Mutex::new(HashMap::new());
}

const QWEN3_RUNTIME_DLL: &str = "sgt_qwen3_runtime.dll";
const QWEN3_RUNTIME_MANIFEST: &str = "sgt_qwen3_runtime.manifest.json";
const QWEN3_RUNTIME_ABI_VERSION: u32 = 2;
const RUNTIME_DLL_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/native/qwen3_runtime/dist/sgt_qwen3_runtime.dll";
const RUNTIME_MANIFEST_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/native/qwen3_runtime/dist/sgt_qwen3_runtime.manifest.json";
const LIBTORCH_URL: &str =
    "https://download.pytorch.org/libtorch/cu128/libtorch-win-shared-with-deps-2.7.1%2Bcu128.zip";
const NATIVE_IMPLEMENTATION: &str = "reference_rust";
const SGT_QWEN3_STATUS_OK: i32 = 0;
const KV_CACHE_MODE_DENSE_APPEND: &str = "dense_append";
const KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT: &str = "experimental_turboquant";
const KV_CACHE_MODE_LEGACY_PAGED_INT8: &str = "paged_int8";
const QWEN3_RUNTIME_REQUIRED_DLLS: &[&str] = &[
    QWEN3_RUNTIME_DLL,
    "torch_cpu.dll",
    "torch_cuda.dll",
    "c10.dll",
    "c10_cuda.dll",
];
const QWEN3_LIBTORCH_REQUIRED_DLLS: &[&str] =
    &["torch_cpu.dll", "torch_cuda.dll", "c10.dll", "c10_cuda.dll"];

pub const QWEN3_RUNTIME_KV_MODE_EXPERIMENTAL_TURBOQUANT: &str =
    KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT;

type RuntimeVersionFn = unsafe extern "C" fn() -> u32;
type ProbeCudaFn = unsafe extern "C" fn(*mut *const c_char, *mut usize) -> i32;
type CreateRuntimeFn = unsafe extern "C" fn(*const u8, usize, *mut *mut c_void) -> i32;
type DestroyRuntimeFn = unsafe extern "C" fn(*mut c_void) -> i32;
type CreateSessionFn = unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut c_void) -> i32;
type DestroySessionFn = unsafe extern "C" fn(*mut c_void) -> i32;
type AppendPcm16Fn = unsafe extern "C" fn(*mut c_void, *const i16, usize, i32) -> i32;
type StepFn = unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut usize) -> i32;
type LastErrorFn = unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut usize) -> i32;

#[derive(Clone, Copy)]
struct RuntimeExports {
    runtime_version: RuntimeVersionFn,
    probe_cuda: ProbeCudaFn,
    create_runtime: CreateRuntimeFn,
    destroy_runtime: DestroyRuntimeFn,
    create_session: CreateSessionFn,
    destroy_session: DestroySessionFn,
    append_pcm16: AppendPcm16Fn,
    step: StepFn,
    last_error: LastErrorFn,
}

struct RuntimeInner {
    library: Option<Library>,
    _preloaded_cuda: (Option<Library>, Option<Library>),
    exports: RuntimeExports,
    handle: *mut c_void,
}

pub struct Qwen3Runtime {
    inner: Rc<RuntimeInner>,
}

pub struct Qwen3Session {
    inner: Rc<RuntimeInner>,
    handle: *mut c_void,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuntimeTranscriptionResult {
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub fixed_text: String,
    #[serde(default)]
    pub draft_text: String,
    #[serde(default)]
    pub session_epoch: u64,
    #[serde(default)]
    pub context_prefix_text: String,
    #[serde(default)]
    pub resume_prefix_text: String,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub audio_samples: usize,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub kv_cache_bytes: usize,
    #[serde(default)]
    pub kv_cache_dense_bytes: usize,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ProbeResponse {
    #[serde(default)]
    implementation: String,
    #[serde(default)]
    quant_mode: String,
    #[serde(default)]
    kv_cache_mode: String,
    #[serde(default)]
    supported_kv_cache_modes: Vec<String>,
    #[serde(default)]
    kv_compression_available: bool,
    #[serde(default)]
    cuda_devices: usize,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct RuntimeDownloadManifest {
    sha256: String,
    abi_version: u32,
    size: u64,
}

#[derive(Debug, Clone)]
struct RuntimeAbiProbe {
    size: u64,
    modified: Option<SystemTime>,
    abi_version: Option<u32>,
}

impl Drop for RuntimeInner {
    fn drop(&mut self) {
        crate::log_info!("[Qwen3Runtime] RuntimeInner drop begin");
        if !self.handle.is_null() {
            crate::log_info!("[Qwen3Runtime] destroy_runtime begin");
            unsafe {
                let _ = (self.exports.destroy_runtime)(self.handle);
            }
            self.handle = std::ptr::null_mut();
            crate::log_info!("[Qwen3Runtime] destroy_runtime complete");
        }
        let aggressive_cuda_reset = should_aggressively_reset_cuda_on_drop();
        if aggressive_cuda_reset {
            crate::log_info!("[Qwen3Runtime] aggressive cudaDeviceReset enabled for teardown");
            reset_cuda_device();
            crate::log_info!("[Qwen3Runtime] aggressive cudaDeviceReset complete");
        } else {
            crate::log_info!("[Qwen3Runtime] skipping cudaDeviceReset on drop");
        }
        let (c10_cuda, torch_cuda) = (self._preloaded_cuda.0.take(), self._preloaded_cuda.1.take());
        let library = self.library.take();
        crate::log_info!("[Qwen3Runtime] unloading qwen runtime libraries");
        drop(c10_cuda);
        drop(torch_cuda);
        drop(library);
        crate::log_info!("[Qwen3Runtime] RuntimeInner drop complete");
    }
}

impl Drop for Qwen3Session {
    fn drop(&mut self) {
        crate::log_info!("[Qwen3Runtime] destroy_session begin");
        if !self.handle.is_null() {
            unsafe {
                let _ = (self.inner.exports.destroy_session)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
        crate::log_info!("[Qwen3Runtime] destroy_session complete");
    }
}

fn set_runtime_notice(message: impl Into<String>) {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = Some(message.into());
}

#[cfg(target_os = "windows")]
fn reset_cuda_device() {
    type CudaDeviceFn = unsafe extern "C" fn() -> i32;

    unsafe {
        unsafe extern "system" {
            fn GetModuleHandleA(lp_module_name: *const u8) -> *mut c_void;
            fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut c_void;
            fn FreeLibrary(h_module: *mut c_void) -> i32;
            fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const u8) -> *mut c_void;
        }

        let mut module = GetModuleHandleA(c"cudart64_12.dll".as_ptr() as *const u8);
        let loaded_here = module.is_null();
        if loaded_here {
            module = LoadLibraryA(c"cudart64_12.dll".as_ptr() as *const u8);
        }
        if !module.is_null() {
            let sync_proc = GetProcAddress(module, c"cudaDeviceSynchronize".as_ptr() as *const u8);
            if !sync_proc.is_null() {
                let sync: CudaDeviceFn = std::mem::transmute(sync_proc);
                let _ = sync();
            }
            let proc = GetProcAddress(module, c"cudaDeviceReset".as_ptr() as *const u8);
            if !proc.is_null() {
                let reset: CudaDeviceFn = std::mem::transmute(proc);
                let _ = reset();
            }
            if loaded_here {
                let _ = FreeLibrary(module);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn reset_cuda_device() {}

fn should_aggressively_reset_cuda_on_drop() -> bool {
    std::env::var("SGT_QWEN3_AGGRESSIVE_CUDA_RESET_ON_DROP")
        .ok()
        .or_else(|| std::env::var("QWEN3_AGGRESSIVE_CUDA_RESET_ON_DROP").ok())
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn clear_runtime_notice() {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = None;
}

pub fn current_qwen3_runtime_notice() -> Option<String> {
    LAST_QWEN3_RUNTIME_NOTICE.lock().ok()?.clone()
}

fn sync_runtime_badge(progress: f32) {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    let (title, message) = if let Ok(state) = REALTIME_STATE.lock() {
        let title = if state.download_title.trim().is_empty() {
            "Downloading...".to_string()
        } else {
            state.download_title.clone()
        };
        (title, state.download_message.clone())
    } else {
        ("Downloading...".to_string(), String::new())
    };

    crate::overlay::auto_copy_badge::show_progress_notification(&title, &message, progress);
}

fn qwen3_runtime_dir_is_usable(dir: &Path) -> bool {
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

fn qwen3_libtorch_required_files_present(dir: &Path) -> bool {
    QWEN3_LIBTORCH_REQUIRED_DLLS
        .iter()
        .all(|name| dir.join(name).is_file())
}

fn runtime_manifest_path(dir: &Path) -> PathBuf {
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

fn fetch_runtime_download_manifest() -> Result<RuntimeDownloadManifest> {
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

fn verify_runtime_dll_against_manifest(
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

static RUNTIME_DOWNLOAD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_qwen3_runtime_downloading() -> bool {
    RUNTIME_DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn download_qwen3_runtime(
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    let capability = crate::runtime_support::supports_qwen3_local_runtime();
    if !capability.is_supported() {
        set_runtime_notice(capability.details.clone());
        if use_badge {
            crate::runtime_support::notify_capability_issue(&capability);
        }
        return Err(anyhow!(capability.details));
    }

    if is_qwen3_runtime_managed_installed() {
        return Ok(());
    }

    // If another thread is already downloading, show its progress and wait
    if RUNTIME_DOWNLOAD_IN_PROGRESS
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        while RUNTIME_DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
        }
        if is_qwen3_runtime_managed_installed() {
            return Ok(());
        }
        return Err(anyhow!("Runtime download did not complete successfully"));
    }

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = "Downloading Qwen3-ASR CUDA Runtime".to_string();
        state.download_message =
            "Please wait... runtime install may download/extract ~2.5 GB.".to_string();
        state.download_progress = 0.0;
    }
    clear_runtime_notice();
    if use_badge {
        sync_runtime_badge(0.0);
    }

    fn post_download_state() {
        use crate::overlay::realtime_webview::state::REALTIME_HWND;
        unsafe {
            if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(REALTIME_HWND),
                    super::super::WM_DOWNLOAD_PROGRESS,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    post_download_state();

    let result: anyhow::Result<()> = (|| {
        let bin_dir = crate::unpack_dlls::private_bin_dir();
        std::fs::create_dir_all(&bin_dir)?;
        let runtime_dll_path = bin_dir.join(QWEN3_RUNTIME_DLL);
        let runtime_manifest = fetch_runtime_download_manifest()?;
        let local_manifest_path = runtime_manifest_path(&bin_dir);

        // Step 1: Download our DLL from the repo
        if verify_runtime_dll_against_manifest(&runtime_dll_path, &runtime_manifest).is_err() {
            let _ = std::fs::remove_file(&runtime_dll_path);
            let _ = std::fs::remove_file(&local_manifest_path);
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = format!("Downloading {}...", QWEN3_RUNTIME_DLL);
                state.download_progress = 0.0;
            }
            post_download_state();
            if use_badge {
                sync_runtime_badge(0.0);
            }

            crate::api::realtime_audio::model_loader::download_file_with_progress(
                RUNTIME_DLL_URL,
                &runtime_dll_path,
                &stop_signal,
                |downloaded, total| {
                    let progress = if total > 0 {
                        (downloaded as f32 / total as f32) * 5.0
                    } else {
                        0.0
                    };
                    if let Ok(mut state) = REALTIME_STATE.lock() {
                        state.download_progress = progress;
                    }
                    if use_badge {
                        sync_runtime_badge(progress);
                    }
                },
            )?;
            verify_runtime_dll_against_manifest(&runtime_dll_path, &runtime_manifest)?;
            std::fs::write(
                &local_manifest_path,
                serde_json::to_vec_pretty(&runtime_manifest)?,
            )?;
        } else if !local_manifest_path.exists() {
            std::fs::write(
                &local_manifest_path,
                serde_json::to_vec_pretty(&runtime_manifest)?,
            )?;
        }

        if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        // Step 2: Download libtorch from pytorch.org if needed
        if !qwen3_libtorch_required_files_present(&bin_dir) {
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message =
                    "Downloading libtorch CUDA runtime (~2.5 GB)...".to_string();
            }
            post_download_state();

            // Use curl as a background process for the large libtorch download
            let libtorch_zip_path = bin_dir.join("libtorch-download.zip");
            let _ = std::fs::remove_file(&libtorch_zip_path);
            let mut curl_child = std::process::Command::new("curl.exe")
                .args([
                    "--fail",
                    "--location",
                    "--continue-at",
                    "-",
                    "--output",
                    &libtorch_zip_path.to_string_lossy(),
                    LIBTORCH_URL,
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|err| anyhow!("Failed to start curl for libtorch download: {err}"))?;

            // Poll curl and update progress based on file size
            let expected_size: u64 = 2_660_000_000; // ~2.5 GB
            loop {
                match curl_child.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            return Err(anyhow!(
                                "libtorch download failed (curl exit code {})",
                                status
                            ));
                        }
                        break;
                    }
                    Ok(None) => {
                        // Still running — update progress from file size
                        if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                            let _ = curl_child.kill();
                            let _ = std::fs::remove_file(&libtorch_zip_path);
                            return Err(anyhow!("Download cancelled"));
                        }
                        let current_size = std::fs::metadata(&libtorch_zip_path)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        let pct = (current_size as f64 / expected_size as f64 * 100.0).min(99.0);
                        let mb = current_size as f64 / 1_048_576.0;
                        let msg = format!("Downloading libtorch... {:.0} MB / ~2500 MB", mb);
                        let progress = pct as f32;
                        if let Ok(mut state) = REALTIME_STATE.lock() {
                            state.download_message = msg.clone();
                            state.download_progress = progress;
                        }
                        if use_badge {
                            sync_runtime_badge(progress);
                        }
                        post_download_state();
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    Err(err) => return Err(anyhow!("Failed to check curl status: {err}")),
                }
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = "Extracting libtorch DLLs...".to_string();
            }
            if use_badge {
                sync_runtime_badge(80.0);
            }
            post_download_state();

            // Extract only DLLs from libtorch/lib/ into bin_dir
            let file = std::fs::File::open(&libtorch_zip_path)?;
            let mut zip = zip::ZipArchive::new(file)
                .map_err(|err| anyhow!("Failed to open libtorch archive: {err}"))?;
            let total_entries = zip.len();
            let mut extracted = 0usize;
            for idx in 0..total_entries {
                let mut entry = zip
                    .by_index(idx)
                    .map_err(|err| anyhow!("Failed to read libtorch archive entry: {err}"))?;
                let name = match entry.enclosed_name() {
                    Some(path) => path.to_path_buf(),
                    None => continue,
                };
                if entry.is_dir() {
                    continue;
                }
                let name_str = name.to_string_lossy();
                if (name_str.contains("/lib/") || name_str.contains("\\lib\\"))
                    && let Some(file_name) = name.file_name()
                    && file_name.to_string_lossy().ends_with(".dll")
                {
                    extracted += 1;
                    let msg = format!(
                        "Extracting DLL {}/~50: {}",
                        extracted,
                        file_name.to_string_lossy()
                    );
                    let progress = 80.0 + (extracted as f32 / 50.0) * 20.0;
                    if let Ok(mut state) = REALTIME_STATE.lock() {
                        state.download_message = msg.clone();
                        state.download_progress = progress;
                    }
                    if use_badge {
                        sync_runtime_badge(progress);
                    }
                    post_download_state();
                    let output_path = bin_dir.join(file_name);
                    let mut output = std::fs::File::create(&output_path)?;
                    std::io::copy(&mut entry, &mut output)?;
                }
            }
            let _ = std::fs::remove_file(libtorch_zip_path);
        }

        if !qwen3_runtime_dir_is_usable(&bin_dir)
            || verify_runtime_dll_against_manifest(&runtime_dll_path, &runtime_manifest).is_err()
        {
            return Err(anyhow!(
                "Qwen3 runtime install is incomplete or ABI-incompatible after download"
            ));
        }

        Ok(())
    })();

    RUNTIME_DOWNLOAD_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_runtime_notice(err.to_string());
        }
    } else {
        clear_runtime_notice();
    }

    result
}

fn runtime_dll_path() -> Result<PathBuf> {
    if let Some(dir) = active_qwen3_runtime_dir() {
        return Ok(dir.join(QWEN3_RUNTIME_DLL));
    }

    let exe = std::env::current_exe().map_err(|err| {
        anyhow!("Failed to locate current executable for Qwen3 runtime lookup: {err}")
    })?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow!("Current executable has no parent directory"))?;
    Ok(parent.join(QWEN3_RUNTIME_DLL))
}

pub fn active_qwen3_runtime_dir() -> Option<PathBuf> {
    runtime_dll_candidates()
        .ok()?
        .into_iter()
        .filter_map(|path| path.parent().map(|parent| parent.to_path_buf()))
        .find(|dir| qwen3_runtime_dir_is_usable(dir))
}

pub fn has_discoverable_qwen3_runtime() -> bool {
    active_qwen3_runtime_dir().is_some()
}

fn runtime_dll_candidates() -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        candidates.push(parent.join(QWEN3_RUNTIME_DLL));
    }

    candidates.push(crate::unpack_dlls::private_bin_dir().join(QWEN3_RUNTIME_DLL));

    if let Ok(repo_root) = repo_root() {
        candidates.push(
            repo_root
                .join("native")
                .join("qwen3_runtime")
                .join("target")
                .join("release")
                .join(QWEN3_RUNTIME_DLL),
        );
        candidates.push(
            repo_root
                .join("dist")
                .join("qwen3-runtime-windows-x64")
                .join(QWEN3_RUNTIME_DLL),
        );
    }

    Ok(candidates)
}

fn repo_root() -> Result<PathBuf> {
    let mut seeds = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        seeds.push(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        seeds.push(parent.to_path_buf());
    }

    for seed in seeds {
        let mut dir = seed;
        loop {
            if dir.join("Cargo.toml").exists() && dir.join(".claude").exists() {
                return Ok(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    Err(anyhow!(
        "Failed to discover repository root for Qwen3 runtime lookup"
    ))
}

fn ensure_cuda_driver_loaded() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HMODULE;
        use windows::Win32::System::LibraryLoader::LoadLibraryA;
        use windows::core::PCSTR;

        let _module: HMODULE = unsafe { LoadLibraryA(PCSTR(c"nvcuda.dll".as_ptr() as *const u8))? };
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(anyhow!("Qwen3 is only supported on Windows"))
    }
}

fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
    let symbol = unsafe { library.get::<T>(name) }.with_context(|| {
        format!(
            "Failed to load runtime symbol '{}'",
            String::from_utf8_lossy(name)
        )
    })?;
    Ok(*symbol)
}

fn decode_json_ptr(ptr: *const c_char, len: usize) -> Result<String> {
    if ptr.is_null() {
        bail!("Qwen3 runtime returned a null JSON pointer");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    Ok(String::from_utf8(bytes.to_vec())?)
}

fn read_last_error_json(exports: &RuntimeExports, handle: *mut c_void) -> String {
    let mut ptr = std::ptr::null();
    let mut len = 0usize;
    let status = unsafe { (exports.last_error)(handle, &mut ptr, &mut len) };
    if status != SGT_QWEN3_STATUS_OK || ptr.is_null() {
        return "Qwen3 runtime did not return an error payload.".to_string();
    }
    decode_json_ptr(ptr, len)
        .unwrap_or_else(|_| "Qwen3 runtime returned an invalid error payload.".to_string())
}

fn status_to_result(
    status: i32,
    exports: &RuntimeExports,
    handle: *mut c_void,
    context: &str,
) -> Result<()> {
    if status == SGT_QWEN3_STATUS_OK {
        return Ok(());
    }
    let last_error = read_last_error_json(exports, handle);
    let message = format!("{context}: {last_error}");
    set_runtime_notice(&message);
    Err(anyhow!(message))
}

fn resolve_requested_kv_cache_mode(requested_override: Option<&str>) -> Result<String> {
    let requested = requested_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            std::env::var("SGT_QWEN3_RUNTIME_KV_MODE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        });

    match requested.as_deref() {
        None | Some(KV_CACHE_MODE_DENSE_APPEND) => Ok(KV_CACHE_MODE_DENSE_APPEND.to_string()),
        Some(KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT) | Some(KV_CACHE_MODE_LEGACY_PAGED_INT8) => {
            Ok(KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT.to_string())
        }
        Some(other) => Err(anyhow!(
            "Invalid SGT_QWEN3_RUNTIME_KV_MODE='{}'. Expected '{}', '{}' or '{}'.",
            other,
            KV_CACHE_MODE_DENSE_APPEND,
            KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT,
            KV_CACHE_MODE_LEGACY_PAGED_INT8
        )),
    }
}

fn canonical_kv_cache_mode_name(value: &str) -> &str {
    match value {
        KV_CACHE_MODE_LEGACY_PAGED_INT8 => KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT,
        other => other,
    }
}

fn runtime_config_json(model_dir: &std::path::Path, kv_cache_mode: &str) -> String {
    json!({
        "model_dir": model_dir.display().to_string(),
        "quant_mode": "reference_uncompressed",
        "kv_cache_mode": kv_cache_mode,
        "streaming_mode": "qwen_reference"
    })
    .to_string()
}

fn session_config_json(
    chunk_ms: u32,
    unfixed_chunks: usize,
    unfixed_tokens: usize,
    language: Option<&str>,
    resume_prefix_text: Option<&str>,
) -> String {
    json!({
        "sample_rate_hz": 16_000,
        "chunk_size_ms": chunk_ms,
        "unfixed_chunk_num": unfixed_chunks,
        "unfixed_token_num": unfixed_tokens,
        "language": language.unwrap_or_default(),
        "resume_prefix_text": resume_prefix_text.unwrap_or_default(),
    })
    .to_string()
}

fn validate_probe_capabilities(probe: &ProbeResponse, requested_kv_cache_mode: &str) -> Result<()> {
    let requested_kv_cache_mode = canonical_kv_cache_mode_name(requested_kv_cache_mode);
    let probe_kv_cache_mode = canonical_kv_cache_mode_name(&probe.kv_cache_mode);
    let supported_kv_cache_modes: Vec<&str> = probe
        .supported_kv_cache_modes
        .iter()
        .map(|mode| canonical_kv_cache_mode_name(mode))
        .collect();

    if probe.implementation != NATIVE_IMPLEMENTATION {
        let message = if probe.implementation.is_empty() {
            "Qwen3 runtime did not report a native implementation identity.".to_string()
        } else {
            format!(
                "Qwen3 runtime reported unsupported implementation '{}'.",
                probe.implementation
            )
        };
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if probe.quant_mode != "reference_uncompressed" {
        let message = if probe.quant_mode.is_empty() {
            "Qwen3 runtime did not report a quant_mode.".to_string()
        } else {
            format!(
                "Qwen3 runtime reported unsupported quant_mode '{}'.",
                probe.quant_mode
            )
        };
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if !probe.kv_compression_available {
        let message = "Qwen3 runtime did not advertise KV compression support.".to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if probe.cuda_devices == 0 {
        let message = "Qwen3 runtime reported no CUDA devices.".to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if probe.supported_kv_cache_modes.is_empty() {
        let message =
            "Qwen3 runtime did not report any supported kv_cache_mode values.".to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if !supported_kv_cache_modes.contains(&requested_kv_cache_mode) {
        let message = format!(
            "Qwen3 runtime does not support kv_cache_mode '{}'. Runtime supports [{}].",
            requested_kv_cache_mode,
            probe.supported_kv_cache_modes.join(", ")
        );
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if probe_kv_cache_mode.is_empty() {
        let message = "Qwen3 runtime did not report an active kv_cache_mode.".to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    Ok(())
}

impl Qwen3Runtime {
    pub fn load(model_dir: &std::path::Path) -> Result<Self> {
        Self::load_with_kv_cache_mode(model_dir, None)
    }

    pub fn load_with_kv_cache_mode(
        model_dir: &std::path::Path,
        kv_cache_mode_override: Option<&str>,
    ) -> Result<Self> {
        if let Err(err) = ensure_cuda_driver_loaded() {
            set_runtime_notice(
                "NVIDIA CUDA driver not available. Qwen3 requires an NVIDIA GPU on Windows.",
            );
            return Err(err);
        }

        let requested_kv_cache_mode = match resolve_requested_kv_cache_mode(kv_cache_mode_override)
        {
            Ok(mode) => mode,
            Err(err) => {
                let message = err.to_string();
                set_runtime_notice(&message);
                return Err(err);
            }
        };

        let dll_path = runtime_dll_path()?;
        if !dll_path.exists() {
            let message = format!("Missing Qwen3 runtime DLL: {}", dll_path.display());
            set_runtime_notice(&message);
            return Err(anyhow!(message));
        }

        // Pre-load libtorch CUDA DLLs before loading our runtime DLL. Libtorch
        // caches CUDA availability during initialization; if torch_cuda.dll is
        // already in-process, it will be found via GetModuleHandle rather than a
        // LoadLibrary call that can fail under the loader lock.
        let _preloaded_cuda = if let Some(dll_dir) = dll_path.parent() {
            let dir_wide: Vec<u16> = dll_dir
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
                let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
            }
            // Pre-load torch_cuda + c10_cuda so they're in-process before libtorch init
            let c10_cuda = unsafe { Library::new(dll_dir.join("c10_cuda.dll")) }.ok();
            let torch_cuda = unsafe { Library::new(dll_dir.join("torch_cuda.dll")) }.ok();
            (c10_cuda, torch_cuda)
        } else {
            (None, None)
        };

        let library = unsafe {
            Library::new(&dll_path).map_err(|err| {
                let message = format!(
                    "Failed to load Qwen3 runtime '{}': {}",
                    dll_path.display(),
                    err
                );
                set_runtime_notice(&message);
                anyhow!(message)
            })?
        };

        let exports = RuntimeExports {
            runtime_version: load_symbol(&library, b"sgt_qwen3_runtime_version\0")?,
            probe_cuda: load_symbol(&library, b"sgt_qwen3_probe_cuda\0")?,
            create_runtime: load_symbol(&library, b"sgt_qwen3_create_runtime\0")?,
            destroy_runtime: load_symbol(&library, b"sgt_qwen3_destroy_runtime\0")?,
            create_session: load_symbol(&library, b"sgt_qwen3_create_session\0")?,
            destroy_session: load_symbol(&library, b"sgt_qwen3_destroy_session\0")?,
            append_pcm16: load_symbol(&library, b"sgt_qwen3_append_pcm16\0")?,
            step: load_symbol(&library, b"sgt_qwen3_step\0")?,
            last_error: load_symbol(&library, b"sgt_qwen3_last_error\0")?,
        };

        let version = unsafe { (exports.runtime_version)() };
        if version != QWEN3_RUNTIME_ABI_VERSION {
            let message = format!(
                "Qwen3 runtime ABI version mismatch: expected {}, got {}.",
                QWEN3_RUNTIME_ABI_VERSION, version
            );
            set_runtime_notice(&message);
            return Err(anyhow!(message));
        }

        let mut probe_ptr = std::ptr::null();
        let mut probe_len = 0usize;
        let probe_status = unsafe { (exports.probe_cuda)(&mut probe_ptr, &mut probe_len) };
        if probe_status != SGT_QWEN3_STATUS_OK {
            let message =
                decode_json_ptr(probe_ptr, probe_len).unwrap_or_else(|err| err.to_string());
            set_runtime_notice(&message);
            return Err(anyhow!("Qwen3 runtime probe failed: {message}"));
        }

        crate::log_info!("[Qwen3Runtime] DLL loaded from: {}", dll_path.display());
        let probe_json = decode_json_ptr(probe_ptr, probe_len)
            .context("Qwen3 runtime returned an invalid probe payload")?;
        let probe: ProbeResponse = serde_json::from_str(&probe_json)
            .with_context(|| format!("Failed to parse Qwen3 probe payload: {probe_json}"))?;
        validate_probe_capabilities(&probe, &requested_kv_cache_mode)?;
        crate::log_info!(
            "[Qwen3Runtime] CUDA ready, kv_cache_mode={}",
            requested_kv_cache_mode
        );
        let config_json = runtime_config_json(model_dir, &requested_kv_cache_mode);
        let mut runtime_handle = std::ptr::null_mut();
        let create_status = unsafe {
            (exports.create_runtime)(config_json.as_ptr(), config_json.len(), &mut runtime_handle)
        };
        status_to_result(
            create_status,
            &exports,
            runtime_handle,
            "Failed to create Qwen3 runtime",
        )?;

        clear_runtime_notice();
        Ok(Self {
            inner: Rc::new(RuntimeInner {
                library: Some(library),
                _preloaded_cuda,
                exports,
                handle: runtime_handle,
            }),
        })
    }

    pub fn create_session(
        &self,
        chunk_ms: u32,
        unfixed_chunks: usize,
        unfixed_tokens: usize,
    ) -> Result<Qwen3Session> {
        self.create_session_with_language_and_prefix(
            chunk_ms,
            unfixed_chunks,
            unfixed_tokens,
            None,
            None,
        )
    }

    pub fn create_session_with_language(
        &self,
        chunk_ms: u32,
        unfixed_chunks: usize,
        unfixed_tokens: usize,
        language: Option<&str>,
    ) -> Result<Qwen3Session> {
        self.create_session_with_language_and_prefix(
            chunk_ms,
            unfixed_chunks,
            unfixed_tokens,
            language,
            None,
        )
    }

    pub fn create_session_with_language_and_prefix(
        &self,
        chunk_ms: u32,
        unfixed_chunks: usize,
        unfixed_tokens: usize,
        language: Option<&str>,
        resume_prefix_text: Option<&str>,
    ) -> Result<Qwen3Session> {
        let config_json = session_config_json(
            chunk_ms,
            unfixed_chunks,
            unfixed_tokens,
            language,
            resume_prefix_text,
        );
        let mut session_handle = std::ptr::null_mut();
        let status = unsafe {
            (self.inner.exports.create_session)(
                self.inner.handle,
                config_json.as_ptr(),
                config_json.len(),
                &mut session_handle,
            )
        };
        status_to_result(
            status,
            &self.inner.exports,
            session_handle,
            "Failed to create Qwen3 session",
        )?;

        Ok(Qwen3Session {
            inner: Rc::clone(&self.inner),
            handle: session_handle,
        })
    }
}

impl Qwen3Session {
    pub fn append_pcm16(&mut self, samples: &[i16], is_final: bool) -> Result<()> {
        let status = unsafe {
            (self.inner.exports.append_pcm16)(
                self.handle,
                samples.as_ptr(),
                samples.len(),
                i32::from(is_final),
            )
        };
        status_to_result(
            status,
            &self.inner.exports,
            self.handle,
            "Failed to append PCM16 to Qwen3 session",
        )
    }

    pub fn step(&mut self) -> Result<RuntimeTranscriptionResult> {
        let mut ptr = std::ptr::null();
        let mut len = 0usize;
        let status = unsafe { (self.inner.exports.step)(self.handle, &mut ptr, &mut len) };
        let payload = decode_json_ptr(ptr, len)?;
        if status != SGT_QWEN3_STATUS_OK {
            let context = serde_json::from_str::<RuntimeTranscriptionResult>(&payload)
                .ok()
                .filter(|result| !result.error.is_empty())
                .map(|result| result.error)
                .unwrap_or(payload);
            let message = format!("Qwen3 step failed: {context}");
            set_runtime_notice(&message);
            return Err(anyhow!(message));
        }
        let result: RuntimeTranscriptionResult =
            serde_json::from_str(&payload).context("Failed to parse Qwen3 runtime JSON result")?;
        if !result.error.is_empty() {
            let message = format!("Qwen3 step failed: {}", result.error);
            set_runtime_notice(&message);
            bail!(message);
        }
        Ok(result)
    }
}
