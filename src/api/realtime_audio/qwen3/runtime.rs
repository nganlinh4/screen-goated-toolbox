use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::path::PathBuf;
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

fn runtime_locale() -> crate::gui::locale::LocaleText {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
}

pub fn current_qwen3_runtime_notice() -> Option<String> {
    LAST_QWEN3_RUNTIME_NOTICE.lock().ok()?.clone()
}

mod install;
mod loader;

pub use install::{
    download_qwen3_runtime, is_qwen3_runtime_downloading, is_qwen3_runtime_managed_installed,
    qwen3_runtime_installed_size, remove_qwen3_runtime,
};
pub use loader::{active_qwen3_runtime_dir, has_discoverable_qwen3_runtime};
use loader::{
    decode_json_ptr, ensure_cuda_driver_loaded, load_symbol, resolve_requested_kv_cache_mode,
    runtime_config_json, runtime_dll_path, session_config_json, status_to_result,
    validate_probe_capabilities,
};

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
