use eframe::egui;
use lazy_static::lazy_static;
use tray_icon::Icon;

// Wrapper to make Icon thread-safe for lazy_static
struct SafeIcon(Icon);
unsafe impl Send for SafeIcon {}
unsafe impl Sync for SafeIcon {}

lazy_static! {
    static ref TRAY_ICON_DARK: SafeIcon = SafeIcon(load_tray_icon(true));
    static ref TRAY_ICON_LIGHT: SafeIcon = SafeIcon(load_tray_icon(false));
}

fn load_tray_icon(is_system_dark: bool) -> Icon {
    // LOGIC:
    // If System is Dark (Dark Taskbar) -> Use Standard Icon (White)
    // If System is Light (Light Taskbar) -> Use Light Mode Icon (Dark/Colored)

    // FIX: Explicit type annotation &[u8] solves the match error
    let icon_bytes: &[u8] = if is_system_dark {
        include_bytes!("../assets/tray_icon.png")
    } else {
        include_bytes!("../assets/tray_icon-light.png")
    };

    let img = image::load_from_memory(icon_bytes).expect("Failed to load tray icon");
    let img_rgba = img.to_rgba8();
    let (width, height) = img_rgba.dimensions();
    let rgba = img_rgba.into_raw();
    tray_icon::Icon::from_rgba(rgba, width, height).unwrap()
}

// Helper to load raw bytes into Tray Icon format
// is_system_dark: TRUE if Windows is in Dark Mode, FALSE if Light Mode
pub fn get_tray_icon(is_system_dark: bool) -> Icon {
    if is_system_dark {
        TRAY_ICON_DARK.0.clone()
    } else {
        TRAY_ICON_LIGHT.0.clone()
    }
}

// Helper to load raw bytes into Window/Taskbar Icon format
pub fn get_window_icon(is_system_dark: bool) -> egui::IconData {
    let icon_bytes: &[u8] = if is_system_dark {
        include_bytes!("../assets/app-icon-small.png")
    } else {
        include_bytes!("../assets/app-icon-small-light.png")
    };

    let img = image::load_from_memory(icon_bytes).expect("Failed to load app icon");
    let img_rgba = img.to_rgba8();
    let (width, height) = img_rgba.dimensions();

    egui::IconData {
        rgba: img_rgba.into_vec(),
        width,
        height,
    }
}
