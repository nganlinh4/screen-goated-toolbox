use std::ffi::CStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_BAD_LENGTH, ERROR_NO_MORE_FILES, HANDLE, HMODULE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetProcAddress};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::PCSTR;

use super::packages::{ONNX_DLL, ONNX_RUNTIME_VERSION, runtime_health_issue};

const MODULE_SNAPSHOT_ATTEMPTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OnnxRuntimeIdentity {
    pub(crate) dll_path: PathBuf,
    pub(crate) version: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OnnxRuntimeInitError {
    RuntimeUnavailable {
        directory: PathBuf,
        detail: String,
    },
    LoadFailed {
        path: PathBuf,
        detail: String,
    },
    ModuleLookupFailed {
        detail: String,
    },
    MultipleRuntimeModules {
        expected: PathBuf,
        loaded: Vec<PathBuf>,
    },
    IdentityMismatch {
        expected: PathBuf,
        loaded: PathBuf,
    },
    VersionMismatch {
        expected: String,
        loaded: String,
    },
    InvalidRuntimeApi {
        detail: String,
    },
    EnvironmentAlreadyInitialized,
}

impl fmt::Display for OnnxRuntimeInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeUnavailable { directory, detail } => write!(
                formatter,
                "local ONNX runtime at '{}' is unavailable: {detail}",
                directory.display()
            ),
            Self::LoadFailed { path, detail } => {
                write!(
                    formatter,
                    "load ONNX runtime '{}': {detail}",
                    path.display()
                )
            }
            Self::ModuleLookupFailed { detail } => {
                write!(formatter, "inspect loaded ONNX runtime module: {detail}")
            }
            Self::MultipleRuntimeModules { expected, loaded } => write!(
                formatter,
                "loaded ONNX runtime module set does not contain exactly '{}': {:?}",
                expected.display(),
                loaded
            ),
            Self::IdentityMismatch { expected, loaded } => write!(
                formatter,
                "loaded ONNX runtime identity mismatch: expected '{}', loaded '{}'",
                expected.display(),
                loaded.display()
            ),
            Self::VersionMismatch { expected, loaded } => write!(
                formatter,
                "loaded ONNX runtime version mismatch: expected '{expected}', loaded '{loaded}'"
            ),
            Self::InvalidRuntimeApi { detail } => {
                write!(formatter, "loaded ONNX runtime API is invalid: {detail}")
            }
            Self::EnvironmentAlreadyInitialized => write!(
                formatter,
                "ONNX Runtime was initialized before the app-local runtime bootstrap"
            ),
        }
    }
}

impl std::error::Error for OnnxRuntimeInitError {}

fn initialized_identity() -> &'static Mutex<Option<OnnxRuntimeIdentity>> {
    static IDENTITY: OnceLock<Mutex<Option<OnnxRuntimeIdentity>>> = OnceLock::new();
    IDENTITY.get_or_init(|| Mutex::new(None))
}

/// Load and verify SGT's app-local ONNX Runtime before any `ort` value or session API.
///
/// Only successful initialization is cached. A missing runtime may therefore be installed on
/// demand and retried in the same process.
pub(crate) fn ensure_onnx_runtime_initialized() -> anyhow::Result<OnnxRuntimeIdentity> {
    let mut guard = initialized_identity()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(identity) = guard.as_ref() {
        verify_cached_identity(identity).map_err(anyhow::Error::new)?;
        return Ok(identity.clone());
    }

    let identity = initialize_uncached().map_err(anyhow::Error::new)?;
    crate::log_info!(
        "[AI Runtime] ONNX Runtime {} loaded from {}",
        identity.version,
        identity.dll_path.display()
    );
    *guard = Some(identity.clone());
    Ok(identity)
}

fn initialize_uncached() -> Result<OnnxRuntimeIdentity, OnnxRuntimeInitError> {
    let expected_path = expected_runtime_path()?;
    let builder =
        ort::init_from(&expected_path).map_err(|error| OnnxRuntimeInitError::LoadFailed {
            path: expected_path.clone(),
            detail: error.to_string(),
        })?;

    let module = loaded_runtime_module(&expected_path)?;
    let loaded_path = loaded_module_path(module)?;
    let loaded_version = loaded_runtime_version(module)?;
    let identity = verify_runtime_identity(
        &expected_path,
        &loaded_path,
        ONNX_RUNTIME_VERSION,
        &loaded_version,
    )?;
    verify_runtime_api(module)?;

    if !builder.commit() {
        return Err(OnnxRuntimeInitError::EnvironmentAlreadyInitialized);
    }
    Ok(identity)
}

fn expected_runtime_path() -> Result<PathBuf, OnnxRuntimeInitError> {
    let directory = super::super::private_bin_dir();
    if let Some(detail) = runtime_health_issue(&directory) {
        return Err(OnnxRuntimeInitError::RuntimeUnavailable { directory, detail });
    }
    canonical_runtime_path(&directory.join(ONNX_DLL)).map_err(|error| {
        OnnxRuntimeInitError::RuntimeUnavailable {
            directory,
            detail: error.to_string(),
        }
    })
}

fn verify_cached_identity(identity: &OnnxRuntimeIdentity) -> Result<(), OnnxRuntimeInitError> {
    let module = loaded_runtime_module(&identity.dll_path)?;
    let loaded_path = loaded_module_path(module)?;
    let loaded_version = loaded_runtime_version(module)?;
    verify_runtime_identity(
        &identity.dll_path,
        &loaded_path,
        &identity.version,
        &loaded_version,
    )?;
    verify_runtime_api(module)
}

fn loaded_runtime_module(expected_path: &Path) -> Result<HMODULE, OnnxRuntimeInitError> {
    let modules = loaded_onnx_runtime_modules()?;
    let loaded_paths: Vec<_> = modules.iter().map(|(_, path)| path.clone()).collect();
    let index = unique_expected_module_index(expected_path, &loaded_paths)?;
    Ok(modules[index].0)
}

fn unique_expected_module_index(
    expected_path: &Path,
    loaded_paths: &[PathBuf],
) -> Result<usize, OnnxRuntimeInitError> {
    if loaded_paths.len() != 1 || path_key(&loaded_paths[0]) != path_key(expected_path) {
        return Err(OnnxRuntimeInitError::MultipleRuntimeModules {
            expected: expected_path.to_path_buf(),
            loaded: loaded_paths.to_vec(),
        });
    }
    Ok(0)
}

fn loaded_onnx_runtime_modules() -> Result<Vec<(HMODULE, PathBuf)>, OnnxRuntimeInitError> {
    let snapshot = create_module_snapshot()?;
    let result = enumerate_onnx_runtime_modules(snapshot);
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    result
}

fn create_module_snapshot() -> Result<HANDLE, OnnxRuntimeInitError> {
    for attempt in 0..MODULE_SNAPSHOT_ATTEMPTS {
        match unsafe {
            CreateToolhelp32Snapshot(
                TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
                GetCurrentProcessId(),
            )
        } {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if should_retry_module_snapshot(error.code(), attempt) => {
                std::thread::yield_now();
            }
            Err(error) => {
                return Err(OnnxRuntimeInitError::ModuleLookupFailed {
                    detail: format!("create process module snapshot: {error}"),
                });
            }
        }
    }
    unreachable!("the final snapshot attempt returns success or a typed error")
}

fn should_retry_module_snapshot(error: windows::core::HRESULT, attempt: usize) -> bool {
    error == windows::core::HRESULT::from_win32(ERROR_BAD_LENGTH.0)
        && attempt + 1 < MODULE_SNAPSHOT_ATTEMPTS
}

fn enumerate_onnx_runtime_modules(
    snapshot: HANDLE,
) -> Result<Vec<(HMODULE, PathBuf)>, OnnxRuntimeInitError> {
    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Module32FirstW(snapshot, &mut entry) }.map_err(|error| {
        OnnxRuntimeInitError::ModuleLookupFailed {
            detail: format!("enumerate first process module: {error}"),
        }
    })?;
    let mut modules = Vec::new();
    loop {
        let name = utf16_z(&entry.szModule);
        if name.eq_ignore_ascii_case("onnxruntime.dll") {
            let path = loaded_module_path(entry.hModule)?;
            modules.push((entry.hModule, path));
        }
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        if let Err(error) = unsafe { Module32NextW(snapshot, &mut entry) } {
            if error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
                break;
            }
            return Err(OnnxRuntimeInitError::ModuleLookupFailed {
                detail: format!("enumerate next process module: {error}"),
            });
        }
    }
    if modules.is_empty() {
        return Err(OnnxRuntimeInitError::ModuleLookupFailed {
            detail: "onnxruntime.dll is not loaded after init_from".to_string(),
        });
    }
    Ok(modules)
}

fn utf16_z(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn loaded_module_path(module: HMODULE) -> Result<PathBuf, OnnxRuntimeInitError> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(OnnxRuntimeInitError::ModuleLookupFailed {
            detail: "GetModuleFileNameW returned no complete path".to_string(),
        });
    }
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
    canonical_runtime_path(&path).map_err(|error| OnnxRuntimeInitError::ModuleLookupFailed {
        detail: format!("canonicalize loaded module '{}': {error}", path.display()),
    })
}

type OrtGetApiBase = unsafe extern "system" fn() -> *const ort::sys::OrtApiBase;

fn runtime_api_base(module: HMODULE) -> Result<*const ort::sys::OrtApiBase, OnnxRuntimeInitError> {
    let symbol = unsafe { GetProcAddress(module, PCSTR(c"OrtGetApiBase".as_ptr().cast())) }
        .ok_or_else(|| OnnxRuntimeInitError::InvalidRuntimeApi {
            detail: "OrtGetApiBase export is missing".to_string(),
        })?;
    let get_api_base: OrtGetApiBase = unsafe { std::mem::transmute(symbol) };
    let base = unsafe { get_api_base() };
    if base.is_null() {
        return Err(OnnxRuntimeInitError::InvalidRuntimeApi {
            detail: "OrtGetApiBase returned null".to_string(),
        });
    }
    Ok(base)
}

fn loaded_runtime_version(module: HMODULE) -> Result<String, OnnxRuntimeInitError> {
    let base = runtime_api_base(module)?;
    let version_ptr = unsafe { ((*base).GetVersionString)() };
    if version_ptr.is_null() {
        return Err(OnnxRuntimeInitError::InvalidRuntimeApi {
            detail: "GetVersionString returned null".to_string(),
        });
    }
    let version = unsafe { CStr::from_ptr(version_ptr) }
        .to_str()
        .map_err(|error| OnnxRuntimeInitError::InvalidRuntimeApi {
            detail: format!("GetVersionString returned invalid UTF-8: {error}"),
        })?;
    Ok(version.to_string())
}

fn verify_runtime_api(module: HMODULE) -> Result<(), OnnxRuntimeInitError> {
    let base = runtime_api_base(module)?;
    let api = unsafe { ((*base).GetApi)(ort::sys::ORT_API_VERSION) };
    if api.is_null() {
        return Err(OnnxRuntimeInitError::InvalidRuntimeApi {
            detail: format!(
                "runtime does not provide API version {}",
                ort::sys::ORT_API_VERSION
            ),
        });
    }
    Ok(())
}

fn verify_runtime_identity(
    expected_path: &Path,
    loaded_path: &Path,
    expected_version: &str,
    loaded_version: &str,
) -> Result<OnnxRuntimeIdentity, OnnxRuntimeInitError> {
    if path_key(expected_path) != path_key(loaded_path) {
        return Err(OnnxRuntimeInitError::IdentityMismatch {
            expected: expected_path.to_path_buf(),
            loaded: loaded_path.to_path_buf(),
        });
    }
    if loaded_version != expected_version {
        return Err(OnnxRuntimeInitError::VersionMismatch {
            expected: expected_version.to_string(),
            loaded: loaded_version.to_string(),
        });
    }
    Ok(OnnxRuntimeIdentity {
        dll_path: loaded_path.to_path_buf(),
        version: loaded_version.to_string(),
    })
}

fn canonical_runtime_path(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    Ok(PathBuf::from(without_verbatim_prefix(
        &canonical.to_string_lossy(),
    )))
}

fn path_key(path: &Path) -> String {
    without_verbatim_prefix(&path.to_string_lossy())
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn without_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_accepts_equivalent_windows_path_forms() {
        let identity = verify_runtime_identity(
            Path::new(r"C:\Runtime\onnxruntime.dll"),
            Path::new(r"\\?\c:\runtime\onnxruntime.dll"),
            "1.24.2",
            "1.24.2",
        )
        .expect("equivalent paths should match");

        assert_eq!(identity.version, "1.24.2");
    }

    #[test]
    fn identity_rejects_another_loaded_module() {
        let error = verify_runtime_identity(
            Path::new(r"C:\App\bin\onnxruntime.dll"),
            Path::new(r"C:\Windows\System32\onnxruntime.dll"),
            "1.24.2",
            "1.24.2",
        )
        .expect_err("a different module path must fail closed");

        assert!(matches!(
            error,
            OnnxRuntimeInitError::IdentityMismatch { .. }
        ));
    }

    #[test]
    fn identity_rejects_another_runtime_version() {
        let error = verify_runtime_identity(
            Path::new(r"C:\App\bin\onnxruntime.dll"),
            Path::new(r"c:\app\bin\onnxruntime.dll"),
            "1.24.2",
            "1.23.0",
        )
        .expect_err("a different runtime version must fail closed");

        assert!(matches!(
            error,
            OnnxRuntimeInitError::VersionMismatch { .. }
        ));
    }

    #[test]
    fn unc_verbatim_prefix_normalizes_without_losing_the_share() {
        assert_eq!(
            path_key(Path::new(r"\\?\UNC\server\share\onnxruntime.dll")),
            r"\\server\share\onnxruntime.dll"
        );
    }

    #[test]
    fn module_set_rejects_a_second_runtime_even_when_expected_is_present() {
        let expected = PathBuf::from(r"C:\App\bin\onnxruntime.dll");
        let loaded = vec![expected.clone(), PathBuf::from(r"C:\Other\onnxruntime.dll")];

        let error = unique_expected_module_index(&expected, &loaded)
            .expect_err("a second ONNX runtime must fail closed");
        assert!(matches!(
            error,
            OnnxRuntimeInitError::MultipleRuntimeModules { .. }
        ));
    }

    #[test]
    fn module_set_rejects_only_the_wrong_runtime() {
        let expected = PathBuf::from(r"C:\App\bin\onnxruntime.dll");
        let loaded = vec![PathBuf::from(r"C:\Other\onnxruntime.dll")];

        let error = unique_expected_module_index(&expected, &loaded)
            .expect_err("a wrong same-named runtime must fail closed");
        assert!(matches!(
            error,
            OnnxRuntimeInitError::MultipleRuntimeModules { .. }
        ));
    }

    #[test]
    fn module_snapshot_retries_only_transient_bad_length_before_last_attempt() {
        let bad_length = windows::core::HRESULT::from_win32(ERROR_BAD_LENGTH.0);
        let other = windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0);

        assert!(should_retry_module_snapshot(bad_length, 0));
        assert!(!should_retry_module_snapshot(
            bad_length,
            MODULE_SNAPSHOT_ATTEMPTS - 1
        ));
        assert!(!should_retry_module_snapshot(other, 0));
    }
}
