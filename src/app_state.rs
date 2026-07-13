use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::config::{Config, load_config};
use crate::history::HistoryManager;
use crate::screen_capture::GdiCapture;
use crate::win_types::SendHwnd;

pub struct AppState {
    pub config: Config,
    pub screenshot_handle: Option<GdiCapture>,
    pub hotkeys_updated: bool,
    pub registered_hotkey_ids: Vec<i32>,
    pub model_usage_stats: HashMap<String, String>,
    pub history: Arc<HistoryManager>,
    pub last_active_window: Option<SendHwnd>,
}

pub static APP: LazyLock<Arc<Mutex<AppState>>> = LazyLock::new(|| {
    Arc::new(Mutex::new({
        let config = load_config();
        let history = Arc::new(HistoryManager::new(config.max_history_items));
        AppState {
            config,
            screenshot_handle: None,
            hotkeys_updated: false,
            registered_hotkey_ids: Vec::new(),
            model_usage_stats: HashMap::new(),
            history,
            last_active_window: None,
        }
    }))
});
