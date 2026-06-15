use super::manager::TtsManager;
use std::sync::{Arc, LazyLock};

/// The global TTS connection manager
pub static TTS_MANAGER: LazyLock<Arc<TtsManager>> = LazyLock::new(|| Arc::new(TtsManager::new()));
