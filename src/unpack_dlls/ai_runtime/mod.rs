use anyhow::{Result, anyhow};
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

mod install;
mod onnx_runtime;
mod packages;
mod progress;

pub(crate) use onnx_runtime::ensure_onnx_runtime_initialized;

use install::install_runtime;
use packages::{
    DIRECTML_DLL, DIRECTML_VERSION, ONNX_DLL, ONNX_RUNTIME_VERSION, ONNX_SHARED_DLL,
    RUNTIME_VERSION_MARKER, core_runtime_present, has_runtime_artifacts, runtime_arch,
    runtime_bytes, runtime_health_issue,
};
use progress::clear_progress;

#[derive(Clone, Debug)]
pub enum AiRuntimeStatus {
    Missing,
    Installing { label: String, progress: f32 },
    Installed { bytes: u64 },
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiRuntimeUi {
    None,
    RealtimeOverlay,
    Badge,
}

static INSTALL_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static STATUS: LazyLock<Mutex<AiRuntimeStatus>> =
    LazyLock::new(|| Mutex::new(AiRuntimeStatus::Missing));
static LAST_ACTION_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(super) fn set_status(status: AiRuntimeStatus) {
    *STATUS.lock().unwrap() = status;
}

fn set_last_action_error(message: impl Into<String>) {
    *LAST_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_last_action_error() {
    *LAST_ACTION_ERROR.lock().unwrap() = None;
}

pub fn current_ai_runtime_notice() -> Option<String> {
    LAST_ACTION_ERROR.lock().unwrap().clone()
}

pub fn is_ai_runtime_installed() -> bool {
    core_runtime_present(&super::private_bin_dir())
}

fn current_ai_runtime_usage_bytes() -> u64 {
    runtime_bytes(&super::private_bin_dir())
}

pub fn current_ai_runtime_status() -> AiRuntimeStatus {
    let status = STATUS.lock().unwrap().clone();
    let bin_dir = super::private_bin_dir();

    match status {
        AiRuntimeStatus::Installing { .. } => status,
        _ if core_runtime_present(&bin_dir) => AiRuntimeStatus::Installed {
            bytes: runtime_bytes(&bin_dir),
        },
        _ if has_runtime_artifacts(&bin_dir) => AiRuntimeStatus::Error(
            runtime_health_issue(&bin_dir)
                .unwrap_or_else(|| "Local AI runtime is invalid. Reinstall required.".to_string()),
        ),
        AiRuntimeStatus::Error(message) => AiRuntimeStatus::Error(message),
        _ => AiRuntimeStatus::Missing,
    }
}

pub fn ai_runtime_version_label() -> String {
    format!(
        "ONNX Runtime {} + DirectML {} ({})",
        ONNX_RUNTIME_VERSION,
        DIRECTML_VERSION,
        runtime_arch()
    )
}

pub fn remove_ai_runtime() -> Result<()> {
    let _guard = INSTALL_MUTEX.lock().unwrap();
    let bin_dir = super::private_bin_dir();

    for name in [
        ONNX_DLL,
        ONNX_SHARED_DLL,
        DIRECTML_DLL,
        RUNTIME_VERSION_MARKER,
    ] {
        let path = bin_dir.join(name);
        if path.exists()
            && let Err(err) = fs::remove_file(&path)
        {
            let message = format!("Failed to remove '{}': {}", path.display(), err);
            set_last_action_error(message.clone());
            return Err(anyhow!(message));
        }
    }

    clear_last_action_error();
    set_status(AiRuntimeStatus::Missing);
    Ok(())
}

pub fn ensure_ai_runtime_installed(stop_signal: Arc<AtomicBool>, ui: AiRuntimeUi) -> Result<()> {
    if is_ai_runtime_installed() {
        clear_last_action_error();
        set_status(AiRuntimeStatus::Installed {
            bytes: current_ai_runtime_usage_bytes(),
        });
        return Ok(());
    }

    let _guard = INSTALL_MUTEX.lock().unwrap();
    if is_ai_runtime_installed() {
        clear_last_action_error();
        set_status(AiRuntimeStatus::Installed {
            bytes: current_ai_runtime_usage_bytes(),
        });
        return Ok(());
    }

    let result = install_runtime(&stop_signal, ui);
    clear_progress(ui);

    match result {
        Ok(()) => {
            clear_last_action_error();
            set_status(AiRuntimeStatus::Installed {
                bytes: current_ai_runtime_usage_bytes(),
            });
            if ui == AiRuntimeUi::Badge {
                let badge = crate::overlay::auto_copy_badge::locale_text();
                crate::overlay::auto_copy_badge::show_detailed_notification(
                    badge.local_ai_runtime_ready,
                    badge.directml_onnx_installed,
                    crate::overlay::auto_copy_badge::NotificationType::Success,
                );
            }
            Ok(())
        }
        Err(err) => {
            if err.to_string().contains("cancelled") {
                set_status(AiRuntimeStatus::Missing);
            } else {
                set_last_action_error(err.to_string());
                set_status(AiRuntimeStatus::Error(err.to_string()));
                if ui != AiRuntimeUi::None {
                    let badge = crate::overlay::auto_copy_badge::locale_text();
                    crate::overlay::auto_copy_badge::show_error_notification(
                        badge.local_ai_runtime_install_failed,
                    );
                }
            }
            Err(err)
        }
    }
}

pub fn start_ai_runtime_install() -> bool {
    if is_ai_runtime_installed()
        || matches!(
            current_ai_runtime_status(),
            AiRuntimeStatus::Installing { .. }
        )
    {
        return false;
    }

    std::thread::spawn(|| {
        let stop_signal = Arc::new(AtomicBool::new(false));
        if let Err(err) = ensure_ai_runtime_installed(stop_signal, AiRuntimeUi::Badge) {
            crate::log_info!("[AI Runtime] Install failed: {err}");
        }
    });
    true
}
