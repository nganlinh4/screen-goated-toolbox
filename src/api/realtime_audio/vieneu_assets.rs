//! Managed VieNeu v2 Turbo backbone and codec model component.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::model_component_assets;
use crate::component_registry::models::{ModelKind, ModelUse};

pub fn get_vieneu_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::Vieneu)
}

pub fn get_vieneu_backbone_dir() -> PathBuf {
    get_vieneu_model_dir().join("backbone")
}

pub fn get_vieneu_decoder_path() -> PathBuf {
    get_vieneu_model_dir()
        .join("codec")
        .join("vieneu_decoder.onnx")
}

pub fn get_vieneu_encoder_path() -> PathBuf {
    get_vieneu_model_dir()
        .join("codec")
        .join("vieneu_encoder.onnx")
}

pub fn is_vieneu_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::Vieneu)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn vieneu_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::Vieneu)
}

pub(crate) fn acquire_vieneu_model() -> Result<ModelUse> {
    model_component_assets::acquire_model(ModelKind::Vieneu)
}

pub fn download_vieneu_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_model_fmt,
        &[("name", "VieNeu v2 Turbo")],
    );
    let message = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_model_fmt,
        &[("name", "VieNeu v2 Turbo")],
    );
    model_component_assets::ensure_model(ModelKind::Vieneu, stop, use_badge, &title, &message)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_vieneu_model() -> Result<()> {
    model_component_assets::remove_model(ModelKind::Vieneu)
}
