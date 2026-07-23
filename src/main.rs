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
mod config;
mod debug_log;
pub mod gui;
mod history;
mod hotkey;
mod icon_gen;
mod initialization;
pub mod lang_detect;
mod model_config;
mod overlay;
mod paths;
mod registry_integration;
mod retry_model_chain;
mod runtime_support;
mod screen_capture;
#[cfg(test)]
mod source_contract_tests;
mod startup_launch;
mod unpack_dlls;
mod updater;
pub mod win_types;

pub use app_activation::RESTORE_EVENT;
pub use app_state::{APP, AppState};
pub(crate) use config::load_config;
pub use screen_capture::GdiCapture;

pub const WINDOW_WIDTH: f32 = 1250.0;
pub const WINDOW_HEIGHT: f32 = 650.0;
// Floor the user can't drag below — keeps the sidebar + editor usable.
pub const MIN_WINDOW_WIDTH: f32 = 1245.0;
pub const MIN_WINDOW_HEIGHT: f32 = 660.0;

fn main() -> eframe::Result<()> {
    app_entry::run()
}

// Re-export hotkey functions for external access.
pub use hotkey::{register_all_hotkeys, unregister_all_hotkeys};
