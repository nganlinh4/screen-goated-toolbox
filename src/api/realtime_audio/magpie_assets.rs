//! Managed NVIDIA Magpie and NanoCodec model component.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use super::model_component_assets;
use crate::component_registry::models::{ModelKind, ModelUse};

const MAGPIE_MODEL_FILE: &str = "magpie_tts_multilingual_357m.nemo";
const NANOCODEC_FILE: &str = "nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo";

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
pub fn current_magpie_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
}

pub fn get_magpie_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::Magpie)
}

pub fn is_magpie_model_downloading() -> bool {
    DOWNLOADING.load(Ordering::Relaxed)
}

pub fn is_magpie_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::Magpie)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn magpie_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::Magpie)
}

pub fn get_magpie_checkpoint_path() -> PathBuf {
    get_magpie_model_dir().join(MAGPIE_MODEL_FILE)
}

pub fn get_magpie_codec_path() -> PathBuf {
    get_magpie_model_dir().join(NANOCODEC_FILE)
}

pub(crate) fn acquire_magpie_model() -> Result<ModelUse> {
    model_component_assets::acquire_model(ModelKind::Magpie)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_magpie_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::Magpie);
    update_notice(&result);
    result
}

pub fn download_magpie_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_magpie_model_downloaded() {
        return Ok(());
    }
    let _downloading = DownloadFlag::acquire(&DOWNLOADING);
    let locale = {
        let app = crate::APP.lock().unwrap();
        crate::gui::locale::LocaleText::get(&app.config.ui_language)
    };
    let result = model_component_assets::ensure_model(
        ModelKind::Magpie,
        stop,
        use_badge,
        locale.tool_runtime.magpie_downloading_title,
        locale.tool_runtime.magpie_downloading_message,
    );
    update_notice(&result);
    result
}

struct DownloadFlag(&'static AtomicBool);

impl DownloadFlag {
    fn acquire(flag: &'static AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for DownloadFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn update_notice(result: &Result<()>) {
    let mut notice = LAST_NOTICE.lock().unwrap();
    *notice = result
        .as_ref()
        .err()
        .filter(|error| !error.to_string().contains("cancelled"))
        .map(ToString::to_string);
}
