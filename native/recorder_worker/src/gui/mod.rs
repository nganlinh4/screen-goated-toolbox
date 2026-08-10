pub mod locale;
mod tool_runtime_locale;
pub mod utils;

pub mod settings_ui {
    pub mod download_manager {
        pub mod ffmpeg_dependency;
    }
}

pub fn request_open_downloaded_tools() {
    crate::overlay::auto_copy_badge::show_detailed_notification(
        "Additional tool required",
        "Open Downloaded Tools in Screen Goated Toolbox.",
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );
}
