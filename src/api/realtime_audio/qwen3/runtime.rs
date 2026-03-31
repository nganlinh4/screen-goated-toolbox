#![allow(dead_code)]

use anyhow::{Result, anyhow};
use libloading::Library;
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref LAST_QWEN3_RUNTIME_NOTICE: Mutex<Option<String>> = Mutex::new(None);
}

const QWEN3_RUNTIME_DLL: &str = "sgt_qwen3_turboquant.dll";
const REQUIRED_EXPORTS: &[&str] = &[
    "sgt_qwen3_runtime_version",
    "sgt_qwen3_probe_cuda",
    "sgt_qwen3_create_session",
    "sgt_qwen3_destroy_session",
];

fn set_runtime_notice(message: impl Into<String>) {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_runtime_notice() {
    *LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap() = None;
}

fn runtime_dll_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|err| anyhow!("Failed to locate current executable for Qwen3 runtime lookup: {err}"))?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow!("Current executable has no parent directory"))?;
    Ok(parent.join(QWEN3_RUNTIME_DLL))
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
        Err(anyhow!("Qwen3 TurboQuant is only supported on Windows"))
    }
}

pub fn current_qwen3_runtime_notice() -> Option<String> {
    LAST_QWEN3_RUNTIME_NOTICE.lock().unwrap().clone()
}

pub fn qwen3_runtime_dll_path() -> Option<PathBuf> {
    runtime_dll_path().ok()
}

pub fn is_qwen3_runtime_present() -> bool {
    runtime_dll_path().is_ok_and(|path| path.exists())
}

pub fn ensure_runtime_ready() -> Result<()> {
    if let Err(err) = ensure_cuda_driver_loaded() {
        set_runtime_notice("NVIDIA CUDA driver not available. Qwen3 TurboQuant requires an NVIDIA GPU on Windows.");
        return Err(err);
    }

    let dll_path = runtime_dll_path()?;
    if !dll_path.exists() {
        let message = format!(
            "Missing Qwen3 TurboQuant runtime DLL: {}",
            dll_path.display()
        );
        set_runtime_notice(&message);
        return Err(anyhow!(message));
    }

    let library = unsafe {
        Library::new(&dll_path).map_err(|err| {
            let message = format!(
                "Failed to load Qwen3 TurboQuant runtime '{}': {}",
                dll_path.display(),
                err
            );
            set_runtime_notice(&message);
            anyhow!(message)
        })?
    };

    for export in REQUIRED_EXPORTS {
        let symbol_name = format!("{export}\0");
        let loaded = unsafe {
            library.get::<unsafe extern "C" fn()>(symbol_name.as_bytes())
        };
        if let Err(err) = loaded {
            let message = format!(
                "Qwen3 TurboQuant runtime is missing required symbol '{}': {}",
                export, err
            );
            set_runtime_notice(&message);
            return Err(anyhow!(message));
        }
    }

    clear_runtime_notice();
    Ok(())
}
