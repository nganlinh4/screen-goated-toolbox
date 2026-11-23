use eframe::egui;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

// --- Clipboard Helper (Existing Code) ---
pub fn copy_to_clipboard_text(text: &str) {
    crate::overlay::utils::copy_to_clipboard(text, HWND::default());
}

// --- Admin Check (Existing Code) ---

#[cfg(target_os = "windows")]
pub fn is_running_as_admin() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut h_token = HANDLE::default();

        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h_token).is_ok() {
            let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
            let mut return_length: u32 = 0;
            let size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;

            let ok = GetTokenInformation(
                h_token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut std::ffi::c_void),
                size,
                &mut return_length,
            )
            .is_ok();
            let _ = CloseHandle(h_token);
            return ok && elevation.TokenIsElevated != 0;
        }
        false
    }
}

#[cfg(test)]
mod elevation_tests {
    use super::is_running_as_admin;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    #[test]
    fn elevation_queries_do_not_accumulate_process_handles() {
        const CHILD: &str = "SGT_ELEVATION_HANDLE_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "gui::utils::elevation_tests::elevation_queries_do_not_accumulate_process_handles",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let count = || {
            let mut count = 0;
            unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) }.unwrap();
            count
        };
        // Warm up thread/process-owned runtime state before measuring the loop.
        let elevated = is_running_as_admin();
        let before = count();
        for _ in 0..4096 {
            assert_eq!(is_running_as_admin(), elevated);
        }
        let after = count();
        eprintln!("elevation queries=4096 handles_before={before} handles_after={after}");
        assert!(after <= before + 2, "handle growth: {before} -> {after}");
    }
}

// --- System Theme Detection ---
/// The Windows "app mode": what application windows should follow.
pub fn is_system_in_dark_mode() -> bool {
    personalize_prefers_dark("AppsUseLightTheme")
}

/// The Windows "mode": the taskbar and tray area the tray icon sits on.
/// It differs from the app mode under the Custom colour setting.
pub fn is_taskbar_in_dark_mode() -> bool {
    personalize_prefers_dark("SystemUsesLightTheme")
}

/// The tray icon must contrast with the taskbar, not with the app window.
pub fn tray_icon_dark(theme_mode: &crate::config::ThemeMode, taskbar_dark: bool) -> bool {
    match theme_mode {
        crate::config::ThemeMode::System => taskbar_dark,
        crate::config::ThemeMode::Dark => true,
        crate::config::ThemeMode::Light => false,
    }
}

/// 0 means dark and 1 means light; a missing value means the Windows default, dark.
fn personalize_prefers_dark(value_name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::*;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) =
            hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
            && let Ok(val) = key.get_value::<u32, &str>(value_name)
        {
            return val == 0;
        }
        true
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = value_name;
        true
    }
}

// --- Font Configuration (Existing Code) ---

/// Downward nudge applied to the monospace face, as a fraction of font size.
///
/// eframe is built with `default-features = false`, so epaint's bundled fonts
/// are absent and `FontDefinitions::default()` starts empty: the monospace
/// family is Consolas alone. Consolas declares an hhea lineGap of 0.171 em,
/// which epaint adds to the galley height *below* the baseline while leaving
/// the baseline at the ascent. Centre-aligning that galley — what every
/// `Layout::*(Align::Center)` row does — therefore lifts the glyphs by half
/// the phantom gap, and monospace runs render ~2px above the proportional text
/// beside them (measured across every row of the usage table).
///
/// The baseline sits this far below the galley centre, per em:
///   Google Sans Flex: ascent 0.9660 - height 1.2520/2 = 0.3403
///   Consolas:         ascent 0.7427 - height 1.1709/2 = 0.1573
/// so shifting Consolas down by the 0.1830 em difference lands both families
/// on one baseline. The shift is purely visual (it does not touch layout) and
/// is absorbed by Consolas' own line gap, so descenders never clip.
pub const MONO_BASELINE_TWEAK: f32 = 0.183;

pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let product_font_name = "google_sans_flex";
    fonts.font_data.insert(
        product_font_name.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(
            crate::assets::GOOGLE_SANS_FLEX_EGUI,
        )),
    );

    let viet_font_name = "segoe_ui";

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let font_dir = std::path::Path::new(&windir).join("Fonts");

    let viet_font_path = font_dir.join("segoeui.ttf");
    let viet_fallback_path = font_dir.join("arial.ttf");
    let viet_data = std::fs::read(&viet_font_path).or_else(|_| std::fs::read(&viet_fallback_path));

    let korean_font_name = "malgun_gothic";
    let korean_font_path = font_dir.join("malgun.ttf");
    let korean_data = std::fs::read(&korean_font_path);

    let mono_font_name = "consolas";
    let mono_data = std::fs::read(font_dir.join("consola.ttf"));
    let mono_tweak = egui::FontTweak {
        y_offset_factor: MONO_BASELINE_TWEAK,
        ..Default::default()
    };
    let emoji_font_name = "segoe_emoji";
    let emoji_data = std::fs::read(font_dir.join("seguiemj.ttf"));

    if let Ok(data) = viet_data {
        fonts.font_data.insert(
            viet_font_name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(data)),
        );
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.insert(0, viet_font_name.to_owned());
        }
    }
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .expect("proportional family exists")
        .insert(0, product_font_name.to_owned());
    if let Ok(data) = korean_data {
        fonts.font_data.insert(
            korean_font_name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(data)),
        );
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            let idx = if vec.contains(&viet_font_name.to_string()) {
                1
            } else {
                0
            };
            vec.insert(idx, korean_font_name.to_owned());
        }
    }
    if let Ok(data) = mono_data {
        fonts.font_data.insert(
            mono_font_name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(data).tweak(mono_tweak)),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("monospace family exists")
            .push(mono_font_name.to_owned());
    }
    if let Ok(data) = emoji_data {
        fonts.font_data.insert(
            emoji_font_name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(data)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .get_mut(&family)
                .expect("built-in family exists")
                .push(emoji_font_name.to_owned());
        }
    }
    if let Some(fallback) = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .and_then(|family| family.first())
        .cloned()
        && fonts
            .families
            .get(&egui::FontFamily::Monospace)
            .is_some_and(Vec::is_empty)
    {
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("monospace family exists")
            .push(fallback);
    }

    ctx.set_fonts(fonts);
}

#[path = "utils/window_icons.rs"]
mod window_icons;

#[path = "utils/theme_change.rs"]
mod theme_change;
pub(crate) use theme_change::{note_setting_change, take_theme_change};

pub fn set_window_icon(hwnd: HWND, is_dark_mode: bool) {
    window_icons::set(hwnd, is_dark_mode);
}

pub fn update_window_icon_native(is_dark_mode: bool) {
    unsafe {
        let hwnd = crate::gui::app::main_window_hwnd();

        if !hwnd.is_invalid() {
            set_window_icon(hwnd, is_dark_mode);
            apply_window_shadow_native(hwnd);
        }
    }
}

pub fn apply_window_shadow_native(hwnd: HWND) {
    unsafe {
        // 1. DWM Shadow (Nice modern shadow)
        // Using only top margin = 1 avoids the dark artifacts at bottom corners
        // while still triggering the system shadow for the whole window.
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 1,
            cyBottomHeight: 0,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // 2. Windows 11 Rounding Preference
        // This ensures the OS understands we want rounded corners, matching the app's internal UI radius.
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        // 3. Force update to apply the shadow and style changes reliably
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(test)]
mod theme_tests {
    use super::tray_icon_dark;
    use crate::config::ThemeMode;

    #[test]
    fn the_tray_icon_follows_the_taskbar_only_in_system_mode() {
        assert!(tray_icon_dark(&ThemeMode::System, true));
        assert!(!tray_icon_dark(&ThemeMode::System, false));
        assert!(tray_icon_dark(&ThemeMode::Dark, false));
        assert!(!tray_icon_dark(&ThemeMode::Light, true));
    }
}
