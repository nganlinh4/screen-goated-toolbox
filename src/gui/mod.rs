pub mod locale;
mod app;
mod key_mapping;
pub mod splash;
pub mod icons;
pub mod settings_ui;
pub mod utils;

pub use app::SettingsApp;
pub use app::signal_restore_window;
pub use utils::configure_fonts;
