use anyhow::Result;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

mod native_loader;
mod progress;

use progress::{clear_progress, update_progress};

pub(crate) use native_loader::ensure_native_onnx_runtime;

#[cfg(feature = "recorder-worker")]
#[derive(Clone, Debug)]
pub enum AiRuntimeStatus {
    Missing,
    Installing,
    Installed,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiRuntimeUi {
    None,
    #[cfg(not(feature = "recorder-worker"))]
    RealtimeOverlay,
    #[cfg(not(feature = "recorder-worker"))]
    Badge,
}

static LAST_ACTION_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub fn is_ai_runtime_installed() -> bool {
    #[cfg(feature = "recorder-worker")]
    return matches!(
        crate::component_registry::local_asr::current_status(
            crate::component_registry::local_asr::ComponentKind::Runtime
        ),
        crate::component_registry::local_asr::ComponentStatus::Installed
    );
    #[cfg(not(feature = "recorder-worker"))]
    matches!(
        crate::component_registry::local_asr::current_status(
            crate::component_registry::local_asr::ComponentKind::Runtime
        ),
        crate::component_registry::local_asr::ComponentStatus::Installed { .. }
    )
}

#[cfg(feature = "recorder-worker")]
pub fn current_ai_runtime_status() -> AiRuntimeStatus {
    use crate::component_registry::local_asr::{ComponentKind, ComponentStatus};
    let status = crate::component_registry::local_asr::current_status(ComponentKind::Runtime);
    if crate::component_registry::local_asr::status_is_ready(&status) {
        return AiRuntimeStatus::Installed;
    }
    match status {
        ComponentStatus::Installing => AiRuntimeStatus::Installing,
        ComponentStatus::Missing | ComponentStatus::Unavailable => AiRuntimeStatus::Missing,
        ComponentStatus::Error => AiRuntimeStatus::Error,
        ComponentStatus::Installed => AiRuntimeStatus::Installed,
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_ai_runtime() -> Result<()> {
    let result = crate::component_registry::local_asr::remove(
        crate::component_registry::local_asr::ComponentKind::Runtime,
    );
    if let Err(error) = &result {
        *LAST_ACTION_ERROR.lock().unwrap() = Some(error.to_string());
    } else {
        *LAST_ACTION_ERROR.lock().unwrap() = None;
    }
    result
}

pub fn ensure_ai_runtime_installed(stop_signal: Arc<AtomicBool>, ui: AiRuntimeUi) -> Result<()> {
    if is_ai_runtime_installed() {
        *LAST_ACTION_ERROR.lock().unwrap() = None;
        return Ok(());
    }
    let result =
        crate::component_registry::local_asr::ensure_runtime(&stop_signal, |done, total| {
            let progress = done
                .saturating_mul(100)
                .checked_div(total.max(1))
                .unwrap_or(0) as f32;
            update_progress(ui, "Installing local ONNX/DirectML runtime", progress);
        });
    clear_progress(ui);
    match result {
        Ok(runtime) => {
            drop(runtime);
            *LAST_ACTION_ERROR.lock().unwrap() = None;
            #[cfg(not(feature = "recorder-worker"))]
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
        Err(error) => {
            if !error.to_string().contains("cancelled") {
                *LAST_ACTION_ERROR.lock().unwrap() = Some(error.to_string());
                #[cfg(not(feature = "recorder-worker"))]
                if ui != AiRuntimeUi::None {
                    let badge = crate::overlay::auto_copy_badge::locale_text();
                    crate::overlay::auto_copy_badge::show_error_notification(
                        badge.local_ai_runtime_install_failed,
                    );
                }
            }
            Err(error)
        }
    }
}

#[cfg(feature = "recorder-worker")]
pub fn start_ai_runtime_install() -> bool {
    crate::component_registry::local_asr::start_install(
        crate::component_registry::local_asr::ComponentKind::Runtime,
    )
}
