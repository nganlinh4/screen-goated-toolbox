pub mod app;
pub mod icons;
mod key_mapping;
pub mod locale;
pub mod model_performance;
pub mod resize_subclass;
pub mod settings_ui;
pub mod splash;
pub mod theme;
pub mod utils;
pub mod widgets;

pub use app::{SettingsApp, SettingsAppInit};
pub use app::{request_open_downloaded_tools, signal_restore_window};
pub use utils::configure_fonts;

use std::sync::LazyLock;

pub static GUI_CONTEXT: LazyLock<std::sync::Mutex<Option<eframe::egui::Context>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));
