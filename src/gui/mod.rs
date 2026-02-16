pub mod app;
pub mod icons;
mod key_mapping;
pub mod locale;
pub mod settings_ui;
pub mod splash;
pub mod utils;

pub use app::signal_restore_window;
pub use app::SettingsApp;
pub use utils::configure_fonts;

lazy_static::lazy_static! {
    pub static ref GUI_CONTEXT: std::sync::Mutex<Option<eframe::egui::Context>> = std::sync::Mutex::new(None);
}
