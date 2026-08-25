use std::sync::LazyLock;

#[path = "../../../../src/overlay/auto_copy_badge/mod.rs"]
pub mod auto_copy_badge;
pub mod html_components;
pub mod realtime_webview;
#[path = "../../../../src/overlay/screen_record/mod.rs"]
pub mod screen_record;
pub mod tts_playground;
pub mod utils;
#[path = "../../../../src/overlay/webview_diagnostics.rs"]
pub mod webview_diagnostics;
#[path = "../../../../src/overlay/webview_init.rs"]
pub mod webview_init;
#[path = "../../../../src/overlay/webview_runtime.rs"]
pub mod webview_runtime;
#[path = "../../../../src/overlay/window_selector/mod.rs"]
pub mod window_selector;

pub static GLOBAL_WEBVIEW_MUTEX: LazyLock<std::sync::Mutex<()>> =
    LazyLock::new(|| std::sync::Mutex::new(()));
