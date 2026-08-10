use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIcon, GetSystemMetrics, ICON_BIG, ICON_SMALL, SM_CXICON, SM_CXSMICON, SM_CYICON,
    SM_CYSMICON, WM_SETICON,
};

fn create_icon(bytes: &[u8], width: i32, height: i32) -> Option<HANDLE> {
    let image = image::load_from_memory(bytes).ok()?.resize_exact(
        width as u32,
        height as u32,
        image::imageops::FilterType::Lanczos3,
    );
    let mut pixels = image.to_rgba8().into_raw();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let mask = vec![0_u8; ((width * height) / 8) as usize];
    let icon =
        unsafe { CreateIcon(None, width, height, 1, 32, mask.as_ptr(), pixels.as_ptr()).ok()? };
    (!icon.is_invalid()).then_some(HANDLE(icon.0))
}

pub fn set_window_icon(hwnd: HWND, dark: bool) {
    let bytes = if dark {
        include_bytes!("../../../../assets/app-icon-small.png").as_slice()
    } else {
        include_bytes!("../../../../assets/app-icon-small-light.png").as_slice()
    };
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        for (kind, width, height) in [
            (
                ICON_SMALL,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
            ),
            (
                ICON_BIG,
                GetSystemMetrics(SM_CXICON),
                GetSystemMetrics(SM_CYICON),
            ),
        ] {
            if let Some(icon) = create_icon(bytes, width, height) {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    WM_SETICON,
                    WPARAM(kind as usize),
                    LPARAM(icon.0 as isize),
                );
            }
        }
    }
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
