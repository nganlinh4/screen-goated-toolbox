use super::{
    KV_CACHE_MODE_DENSE_APPEND, KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT,
    KV_CACHE_MODE_LEGACY_PAGED_INT8, NATIVE_IMPLEMENTATION, ProbeResponse, QWEN3_RUNTIME_DLL,
    RuntimeExports, SGT_QWEN3_STATUS_OK, install::qwen3_runtime_dir_is_usable, set_runtime_notice,
};
use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use serde_json::json;
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};

pub(super) fn runtime_dll_path() -> Result<PathBuf> {
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

pub(super) fn ensure_cuda_driver_loaded() -> Result<()> {
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

pub(super) fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
    let symbol = unsafe { library.get::<T>(name) }.with_context(|| {
        format!(
            "Failed to load runtime symbol '{}'",
            String::from_utf8_lossy(name)
        )
    })?;
    Ok(*symbol)
}

pub(super) fn decode_json_ptr(ptr: *const c_char, len: usize) -> Result<String> {
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

pub(super) fn status_to_result(
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

pub(super) fn resolve_requested_kv_cache_mode(requested_override: Option<&str>) -> Result<String> {
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

pub(super) fn runtime_config_json(model_dir: &Path, kv_cache_mode: &str) -> String {
    json!({
        "model_dir": model_dir.display().to_string(),
        "quant_mode": "reference_uncompressed",
        "kv_cache_mode": kv_cache_mode,
        "streaming_mode": "qwen_reference"
    })
    .to_string()
}

pub(super) fn session_config_json(
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

pub(super) fn validate_probe_capabilities(
    probe: &ProbeResponse,
    requested_kv_cache_mode: &str,
) -> Result<()> {
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
