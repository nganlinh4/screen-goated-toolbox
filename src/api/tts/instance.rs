use super::manager::TtsManager;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    /// The global TTS connection manager
    pub static ref TTS_MANAGER: Arc<TtsManager> = Arc::new(TtsManager::new());
}
