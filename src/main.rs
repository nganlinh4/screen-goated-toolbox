#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The Computer Control tool catalog is one large `json!` literal; its array
// expands recursively, so the default macro recursion limit (128) is too low.
#![recursion_limit = "512"]

pub mod api;
mod app_activation;
mod app_entry;
mod app_state;
mod assets;
mod atomic_json;
#[cfg(test)]
mod catalog_benchmark;
mod component_registry;
mod config;
mod creation_feature_availability;
mod crypto;
mod debug_log;
pub mod gui;
mod history;
mod hotkey;
mod icon_gen;
mod image_decode;
mod initialization;
pub mod lang_detect;
mod model_config;
mod model_feed;
mod overlay;
mod paths;
mod registry_integration;
mod retry_model_chain;
mod runtime_support;
mod screen_capture;
#[cfg(test)]
mod source_contract_tests;
mod startup_launch;
mod task_runtime;
mod unpack_dlls;
mod updater;
mod usage_stats;
pub mod win_types;

pub use app_activation::RESTORE_EVENT;
pub use app_state::{APP, AppState};
pub(crate) use config::load_config;
pub use screen_capture::GdiCapture;

// The footer raises this baseline per locale when its launchers need more room.
pub const MIN_WINDOW_WIDTH: f32 = 1100.0;
pub const MIN_WINDOW_HEIGHT: f32 = 720.0;
pub const WINDOW_WIDTH: f32 = MIN_WINDOW_WIDTH;
pub const WINDOW_HEIGHT: f32 = MIN_WINDOW_HEIGHT;

fn main() -> eframe::Result<std::process::ExitCode> {
    app_entry::run()
}

// Re-export hotkey functions for external access.
pub use hotkey::{register_all_hotkeys, unregister_all_hotkeys};
