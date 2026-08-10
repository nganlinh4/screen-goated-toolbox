#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "512"]

mod api;
#[path = "../../../src/atomic_json.rs"]
mod atomic_json;
#[path = "../../../src/component_registry/mod.rs"]
mod component_registry;
#[path = "../../../src/config/mod.rs"]
mod config;
#[path = "../../../src/debug_log.rs"]
mod debug_log;
mod gui;
mod initialization;
#[path = "../../../src/lang_detect.rs"]
mod lang_detect;
#[path = "../../../src/model_config.rs"]
mod model_config;
mod overlay;
#[path = "../../../src/paths.rs"]
mod paths;
#[path = "../../../src/retry_model_chain.rs"]
mod retry_model_chain;
mod runtime_support;
#[path = "../../../src/unpack_dlls.rs"]
mod unpack_dlls;
mod usage_stats;
mod win_types;

mod hotkey {
    pub const MOD_ALT: u32 = 0x0001;
    pub const MOD_CONTROL: u32 = 0x0002;
    pub const MOD_SHIFT: u32 = 0x0004;
    pub const MOD_WIN: u32 = 0x0008;
}

pub struct AppState {
    pub config: config::Config,
    pub model_usage_stats: usage_stats::UsageStore,
}

pub static APP: std::sync::LazyLock<std::sync::Arc<std::sync::Mutex<AppState>>> =
    std::sync::LazyLock::new(|| {
        std::sync::Arc::new(std::sync::Mutex::new(AppState {
            config: config::load_config(),
            model_usage_stats: usage_stats::UsageStore::new(),
        }))
    });

pub(crate) use config::load_config;

pub const WINDOW_WIDTH: f32 = 1250.0;
pub const WINDOW_HEIGHT: f32 = 650.0;
pub const MIN_WINDOW_WIDTH: f32 = 1245.0;
pub const MIN_WINDOW_HEIGHT: f32 = 660.0;

fn main() {
    if let Err(error) = overlay::screen_record::run_worker() {
        eprintln!("Screen Recorder worker failed: {error:#}");
        std::process::exit(1);
    }
}
