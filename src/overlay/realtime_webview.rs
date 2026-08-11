pub mod app_selection;
pub mod controller;
mod document;
mod ipc;
mod layout;
pub mod manager;
pub mod state;
pub mod webview;
pub mod wndproc;

pub use manager::{is_realtime_overlay_active, show_realtime_overlay, stop_realtime_overlay};
pub use state::*;
