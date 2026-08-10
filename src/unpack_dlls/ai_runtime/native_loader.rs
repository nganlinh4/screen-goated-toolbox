use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW,
};
use windows::core::{PCSTR, PCWSTR};

const EXPECTED_VERSION: &str = "1.24.2";
const DLLS: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "DirectML.dll",
];

static LOADED: LazyLock<Mutex<Option<NativeOnnxRuntime>>> = LazyLock::new(|| Mutex::new(None));

struct NativeOnnxRuntime {
    _modules: Vec<LoadedModule>,
    _runtime: crate::component_registry::local_asr::OnnxRuntimeUse,
    _vc: crate::component_registry::vc_runtime::LoadedVcRuntime,
}

struct LoadedModule(isize);

// The modules are process-global and accessed only through the OS loader.
unsafe impl Send for LoadedModule {}

impl Drop for LoadedModule {
    fn drop(&mut self) {
        let _ = unsafe { FreeLibrary(HMODULE(self.0 as *mut c_void)) };
    }
}

pub(crate) fn ensure_native_onnx_runtime() -> Result<()> {
    let mut loaded = LOADED.lock().unwrap_or_else(|value| value.into_inner());
    if loaded.is_some() {
        return Ok(());
    }
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let vc = crate::component_registry::vc_runtime::ensure_component(|_, _| {})?.preload()?;
    let runtime = crate::component_registry::local_asr::ensure_runtime(&cancelled, |_, _| {})?;
    let mut modules = Vec::with_capacity(DLLS.len());
    for name in DLLS {
        modules.push(load_exact(&runtime.bin_dir().join(name))?);
    }
    verify_runtime_version(&modules[0])?;
    *loaded = Some(NativeOnnxRuntime {
        _modules: modules,
        _runtime: runtime,
        _vc: vc,
    });
    Ok(())
}

fn load_exact(path: &Path) -> Result<LoadedModule> {
    let expected = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize native runtime '{}'", path.display()))?;
    let wide = expected
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let module = unsafe {
        LoadLibraryExW(
            PCWSTR(wide.as_ptr()),
            None,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    }
    .with_context(|| format!("load native runtime '{}'", expected.display()))?;
    let loaded = LoadedModule(module.0 as isize);
    let actual = module_path(module)?;
    if !same_path(&expected, &actual) {
        bail!(
            "native runtime identity mismatch: expected '{}', loaded '{}'",
            expected.display(),
            actual.display()
        );
    }
    Ok(loaded)
}

fn module_path(module: HMODULE) -> Result<PathBuf> {
    let mut capacity = 512_usize;
    loop {
        let mut buffer = vec![0_u16; capacity];
        let length = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
        if length == 0 {
            return Err(anyhow!("inspect loaded native runtime path"));
        }
        if length < buffer.len() - 1 {
            buffer.truncate(length);
            return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)));
        }
        capacity = capacity
            .checked_mul(2)
            .filter(|value| *value <= 32_768)
            .ok_or_else(|| anyhow!("loaded native runtime path exceeds its limit"))?;
    }
}

#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "system" fn(u32) -> *const c_void,
    get_version_string: unsafe extern "system" fn() -> *const c_char,
}

type OrtGetApiBase = unsafe extern "system" fn() -> *const OrtApiBase;

fn verify_runtime_version(module: &LoadedModule) -> Result<()> {
    let function = unsafe {
        GetProcAddress(
            HMODULE(module.0 as *mut c_void),
            PCSTR(c"OrtGetApiBase".as_ptr().cast()),
        )
    }
    .ok_or_else(|| anyhow!("ONNX Runtime has no OrtGetApiBase export"))?;
    let get_api_base: OrtGetApiBase = unsafe { std::mem::transmute(function) };
    let base = unsafe { get_api_base() };
    if base.is_null() {
        bail!("ONNX Runtime returned a null API base");
    }
    let version = unsafe { ((*base).get_version_string)() };
    if version.is_null() {
        bail!("ONNX Runtime returned a null version string");
    }
    let version = unsafe { CStr::from_ptr(version) }
        .to_str()
        .context("ONNX Runtime version is not UTF-8")?;
    if version != EXPECTED_VERSION {
        bail!("ONNX Runtime version mismatch: expected {EXPECTED_VERSION}, loaded {version}");
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
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

use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_identity_is_case_insensitive_but_not_location_agnostic() {
        assert!(same_path(
            Path::new(r"C:\Runtime\onnxruntime.dll"),
            Path::new(r"c:\runtime\ONNXRUNTIME.DLL")
        ));
        assert!(!same_path(
            Path::new(r"C:\Runtime\onnxruntime.dll"),
            Path::new(r"C:\Other\onnxruntime.dll")
        ));
        assert!(same_path(
            Path::new(r"C:\Runtime\onnxruntime.dll"),
            Path::new(r"\\?\c:\runtime\onnxruntime.dll")
        ));
    }
}
