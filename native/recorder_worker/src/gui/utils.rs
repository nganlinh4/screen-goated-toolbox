use windows::Win32::Foundation::HWND;

#[path = "../../../../src/gui/utils/window_icons.rs"]
mod window_icons;

pub fn set_window_icon(hwnd: HWND, dark: bool) {
    window_icons::set(hwnd, dark);
}

pub fn is_system_in_dark_mode() -> bool {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            KEY_READ,
        )
        .ok()
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme").ok())
        .is_none_or(|value| value == 0)
}
