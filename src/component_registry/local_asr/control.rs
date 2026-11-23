//! Settings-side status, background install and removal of the ONNX runtime.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, bail};

use super::super::RemovalOutcome;
use super::validation::validate_delivery_status;
use super::{ARCHITECTURE, RUNTIME_ID, ensure_runtime, optional_delivery, version_root};

#[derive(Clone, Debug)]
pub(crate) enum ComponentStatus {
    Installed { bytes: u64, version: String },
    Installing { progress: f32 },
    Missing,
    Unavailable,
    Error(String),
}

static INSTALLING_RUNTIME: AtomicBool = AtomicBool::new(false);
static RUNTIME_PROGRESS: AtomicU32 = AtomicU32::new(0);
static RUNTIME_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(super) fn is_installing() -> bool {
    INSTALLING_RUNTIME.load(Ordering::Acquire)
}

pub(crate) fn current_status() -> ComponentStatus {
    if is_installing() {
        return ComponentStatus::Installing {
            progress: RUNTIME_PROGRESS.load(Ordering::Relaxed) as f32 / 100.0,
        };
    }
    let Some(delivery) = optional_delivery(RUNTIME_ID) else {
        return ComponentStatus::Unavailable;
    };
    match validate_delivery_status(delivery) {
        Ok(()) => ComponentStatus::Installed {
            bytes: delivery.unpacked_size_bytes,
            version: delivery.version.to_string(),
        },
        Err(error)
            if version_root(delivery.id, delivery.version).is_ok_and(|root| root.exists()) =>
        {
            ComponentStatus::Error(error.to_string())
        }
        Err(_) => ComponentStatus::Missing,
    }
}

pub(crate) fn current_notice() -> Option<String> {
    RUNTIME_NOTICE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clone()
}

pub(crate) fn version_label() -> Option<String> {
    optional_delivery(RUNTIME_ID).map(|delivery| format!("{} ({ARCHITECTURE})", delivery.version))
}

pub(crate) fn start_install() -> bool {
    let ready = matches!(
        current_status(),
        ComponentStatus::Installed { .. } | ComponentStatus::Installing { .. }
    );
    if ready
        || INSTALLING_RUNTIME
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    clear_notice();
    let cancelled = Arc::new(AtomicBool::new(false));
    let Ok(activity) = crate::install_activity::register(cancelled.clone()) else {
        INSTALLING_RUNTIME.store(false, Ordering::Release);
        return false;
    };
    std::thread::spawn(move || {
        let _activity = activity;
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&localized_name());
        let result = ensure_runtime(&cancelled, |done, total| {
            store_progress(done, total);
            badge.report(done, total);
        });
        if let Err(error) = result {
            *RUNTIME_NOTICE
                .lock()
                .unwrap_or_else(|value| value.into_inner()) = Some(error.to_string());
        }
        RUNTIME_PROGRESS.store(0, Ordering::Relaxed);
        INSTALLING_RUNTIME.store(false, Ordering::Release);
    });
    true
}

pub(crate) fn remove() -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    match super::super::request_remove_and_wait(RUNTIME_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => {
            clear_notice();
            Ok(())
        }
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{RUNTIME_ID} is required by installed components: {}",
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{RUNTIME_ID} contains {} unrecorded or unsafe path(s); they were preserved",
            paths.len()
        ),
    }
}

fn localized_name() -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
        .auxiliary
        .managed_tools
        .tool_onnx_runtime
        .to_string()
}

fn clear_notice() {
    *RUNTIME_NOTICE
        .lock()
        .unwrap_or_else(|value| value.into_inner()) = None;
}

fn store_progress(done: u64, total: u64) {
    let basis_points = done
        .saturating_mul(10_000)
        .checked_div(total.max(1))
        .unwrap_or(0);
    RUNTIME_PROGRESS.store(basis_points.min(10_000) as u32, Ordering::Relaxed);
}
