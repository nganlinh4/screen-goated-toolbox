use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use serde::Deserialize;
use serde_json::json;
use std::ffi::{c_char, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    static ref LAST_QWEN3_RUNTIME_NOTICE: Mutex<Option<String>> = Mutex::new(None);
}

const QWEN3_RUNTIME_DLL: &str = "sgt_qwen3_runtime.dll";
const QWEN3_RUNTIME_ABI_VERSION: u32 = 1;
const NATIVE_IMPLEMENTATION: &str = "reference_rust";
const SGT_QWEN3_STATUS_OK: i32 = 0;
const KV_CACHE_MODE_DENSE_APPEND: &str = "dense_append";
const KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT: &str = "experimental_turboquant";
const KV_CACHE_MODE_LEGACY_PAGED_INT8: &str = "paged_int8";

type RuntimeVersionFn = unsafe extern "C" fn() -> u32;
type ProbeCudaFn = unsafe extern "C" fn(*mut *const c_char, *mut usize) -> i32;
type CreateRuntimeFn = unsafe extern "C" fn(*const u8, usize, *mut *mut c_void) -> i32;
type DestroyRuntimeFn = unsafe extern "C" fn(*mut c_void) -> i32;
type CreateSessionFn = unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut c_void) -> i32;
type DestroySessionFn = unsafe extern "C" fn(*mut c_void) -> i32;
type AppendPcm16Fn = unsafe extern "C" fn(*mut c_void, *const i16, usize, i32) -> i32;
type StepFn = unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut usize) -> i32;
type ResetSessionFn = unsafe extern "C" fn(*mut c_void) -> i32;
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
    reset_session: ResetSessionFn,
    last_error: LastErrorFn,
}

struct RuntimeInner {
    _library: Library,
    _preloaded_cuda: (Option<Library>, Option<Library>),
    exports: RuntimeExports,
    handle: *mut c_void,
}

pub struct Qwen3Runtime {
    inner: Arc<RuntimeInner>,
}

pub struct Qwen3Session {
    inner: Arc<RuntimeInner>,
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
    #[serde(alias = "turboquant_kv")]
    kv_compression_available: bool,
    #[serde(default)]
    cuda_devices: usize,
}

impl Drop for RuntimeInner {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                let _ = (self.exports.destroy_runtime)(self.handle);
            }
        }
    }
}

impl Drop for Qwen3Session {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                let _ = (self.inner.exports.destroy_session)(self.handle);
            }
        }
    }
}

fn set_runtime_notice(message: impl Into<String>) {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_runtime_notice() {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = None;
}

fn runtime_dll_path() -> Result<PathBuf> {
    for path in runtime_dll_candidates()? {
        if path.exists() {
            return Ok(path);
        }
    }

    let exe = std::env::current_exe().map_err(|err| {
        anyhow!("Failed to locate current executable for Qwen3 runtime lookup: {err}")
    })?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow!("Current executable has no parent directory"))?;
    Ok(parent.join(QWEN3_RUNTIME_DLL))
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

        let _module: HMODULE = unsafe { LoadLibraryA(PCSTR(b"nvcuda.dll\0".as_ptr()))? };
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
    decode_json_ptr(ptr, len).unwrap_or_else(|_| {
        "Qwen3 runtime returned an invalid error payload.".to_string()
    })
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

fn requested_kv_cache_mode() -> Result<String> {
    let requested = std::env::var("SGT_QWEN3_RUNTIME_KV_MODE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

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

fn session_config_json(chunk_ms: u32, unfixed_chunks: usize, unfixed_tokens: usize) -> String {
    json!({
        "sample_rate_hz": 16_000,
        "chunk_size_ms": chunk_ms,
        "unfixed_chunk_num": unfixed_chunks,
        "unfixed_token_num": unfixed_tokens
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
            "Qwen3 runtime did not report a native implementation identity."
                .to_string()
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
        let message =
            "Qwen3 runtime did not advertise KV compression support.".to_string();
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
            "Qwen3 runtime did not report any supported kv_cache_mode values."
                .to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if !supported_kv_cache_modes
        .iter()
        .any(|mode| *mode == requested_kv_cache_mode)
    {
        let message = format!(
            "Qwen3 runtime does not support kv_cache_mode '{}'. Runtime supports [{}].",
            requested_kv_cache_mode,
            probe.supported_kv_cache_modes.join(", ")
        );
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    if probe_kv_cache_mode.is_empty() {
        let message =
            "Qwen3 runtime did not report an active kv_cache_mode.".to_string();
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }
    Ok(())
}

impl Qwen3Runtime {
    pub fn load(model_dir: &std::path::Path) -> Result<Self> {
        if let Err(err) = ensure_cuda_driver_loaded() {
            set_runtime_notice(
                "NVIDIA CUDA driver not available. Qwen3 requires an NVIDIA GPU on Windows.",
            );
            return Err(err);
        }

        let requested_kv_cache_mode = match requested_kv_cache_mode() {
            Ok(mode) => mode,
            Err(err) => {
                let message = err.to_string();
                set_runtime_notice(&message);
                return Err(err);
            }
        };

        let dll_path = runtime_dll_path()?;
        if !dll_path.exists() {
            let message = format!(
                "Missing Qwen3 runtime DLL: {}",
                dll_path.display()
            );
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
            reset_session: load_symbol(&library, b"sgt_qwen3_reset_session\0")?,
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
        crate::log_info!("[Qwen3Runtime] CUDA ready, kv_cache_mode={}", requested_kv_cache_mode);
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
            inner: Arc::new(RuntimeInner {
                _library: library,
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
        let config_json = session_config_json(chunk_ms, unfixed_chunks, unfixed_tokens);
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
            inner: Arc::clone(&self.inner),
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

    pub fn reset(&mut self) -> Result<()> {
        let status = unsafe { (self.inner.exports.reset_session)(self.handle) };
        status_to_result(
            status,
            &self.inner.exports,
            self.handle,
            "Failed to reset Qwen3 session",
        )
    }
}

pub fn current_qwen3_runtime_notice() -> Option<String> {
    LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap().clone()
}
