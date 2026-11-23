//! Removal entry point for the downloaded ONNX/DirectML runtime, used by the
//! Downloaded Tools "clean everything" action.

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_ai_runtime() -> anyhow::Result<()> {
    crate::component_registry::local_asr::remove()
}
