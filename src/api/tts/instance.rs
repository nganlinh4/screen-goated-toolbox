use std::sync::Arc;
use lazy_static::lazy_static;
use super::manager::TtsManager;

lazy_static! {
    /// The global TTS connection manager
    pub static ref TTS_MANAGER: Arc<TtsManager> = Arc::new(TtsManager::new());
}
