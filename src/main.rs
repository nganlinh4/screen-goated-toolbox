#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The Computer Control tool catalog is one large `json!` literal; its array
// expands recursively, so the default macro recursion limit (128) is too low.
#![recursion_limit = "512"]

mod admin_startup;
mod api;
mod app_activation;
mod app_entry;
mod app_state;
mod assets;
mod atomic_json;
mod audio_input;
#[cfg(test)]
mod catalog_benchmark;
mod com_scope;
mod component_registry;
mod config;
mod crash_report;
mod creation_feature_availability;
mod credential_error;
mod crypto;
mod debug_log;
pub mod gui;
mod history;
mod hotkey;
mod icon_gen;
mod image_decode;
mod initialization;
mod install_activity;
mod install_progress;
pub mod lang_detect;
mod loopback_resource;
mod model_config;
mod model_feed;
#[cfg(windows)]
mod native_file_dialog;
mod overlay;
mod paths;
mod process_job;
mod registry_integration;
mod retry_model_chain;
mod runtime_support;
mod screen_capture;
#[cfg(test)]
mod source_contract_tests;
mod startup_launch;
mod system_command;
mod task_runtime;
mod unpack_dlls;
mod updater;
mod usage_stats;
mod verbatim_path;
mod verified_scratch;
pub mod win_types;

pub use app_activation::RESTORE_EVENT;
pub use app_state::{APP, AppState};
#[cfg(feature = "recorder-worker")]
pub(crate) use config::load_config;
pub use screen_capture::GdiCapture;

// The footer raises this baseline per locale when its launchers need more room.
pub const MIN_WINDOW_WIDTH: f32 = 1000.0;
pub const MIN_WINDOW_HEIGHT: f32 = 720.0;
pub const WINDOW_WIDTH: f32 = MIN_WINDOW_WIDTH;
pub const WINDOW_HEIGHT: f32 = MIN_WINDOW_HEIGHT;

fn main() -> eframe::Result<std::process::ExitCode> {
    paths::isolate_inherited_credentials(&[]);
    let result = app_entry::run();
    crash_report::record_main_result(&result);
    result
}

// Re-export hotkey functions for external access.
pub use hotkey::{register_all_hotkeys, unregister_all_hotkeys};
