use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

pub(crate) fn download_runtime(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    crate::component_registry::creation::download(stop, use_badge)
}
