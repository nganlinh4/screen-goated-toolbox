//! Managed Supertonic 3 model component.

#[cfg(not(feature = "recorder-worker"))]
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use super::model_component_assets;
use crate::component_registry::models::{ModelKind, ModelUse};

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
pub fn current_supertonic_model_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn get_supertonic_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::Supertonic)
}

pub fn is_supertonic_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::Supertonic)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn supertonic_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::Supertonic)
}

pub(crate) fn acquire_supertonic_model() -> Result<ModelUse> {
    model_component_assets::acquire_model(ModelKind::Supertonic)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_supertonic_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::Supertonic);
    update_notice(&result);
    result
}

pub fn download_supertonic_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_model_fmt,
        &[("name", "Supertonic 3")],
    );
    let message = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_model_fmt,
        &[("name", "Supertonic 3")],
    );
    let result = model_component_assets::ensure_model(
        ModelKind::Supertonic,
        stop,
        use_badge,
        &title,
        &message,
    );
    update_notice(&result);
    result
}

fn update_notice(result: &Result<()>) {
    let mut notice = LAST_NOTICE.lock().unwrap();
    *notice = result
        .as_ref()
        .err()
        .filter(|error| !error.to_string().contains("cancelled"))
        .map(ToString::to_string);
}
