// --- INITIALIZATION ---
// Application bootstrap: COM init, dark mode, cleanup, and warmups.

use windows::core::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Enable dark mode for Win32 native menus (context menus, tray menus).
/// Uses undocumented SetPreferredAppMode API from uxtheme.dll.
pub fn enable_dark_mode_for_app() {
    // PreferredAppMode enum values
    const ALLOW_DARK: u32 = 1; // AllowDark mode

    unsafe {
        // Load uxtheme.dll
        if let Ok(uxtheme) = LoadLibraryW(w!("uxtheme.dll")) {
            // SetPreferredAppMode is at ordinal 135 (undocumented)
            let ordinal = 135u16;
            let ordinal_ptr = ordinal as usize as *const u8;
            let proc_name = PCSTR::from_raw(ordinal_ptr);

            if let Some(set_preferred_app_mode) = GetProcAddress(uxtheme, proc_name) {
                // Cast to function pointer: fn(u32) -> u32
                let func: extern "system" fn(u32) -> u32 =
                    std::mem::transmute(set_preferred_app_mode);
                func(ALLOW_DARK);
            }
        }
    }
}

/// Cleanup temporary files left by the application (restart scripts, partial downloads).
pub fn cleanup_temporary_files() {
    // 1. Clean up restart scripts in %TEMP%
    let temp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("sgt_restart_") && name_str.ends_with(".bat") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // 2. Clean up partial downloads in the app's bin directory
    let bin_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin");

    if bin_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "tmp") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    // 3. Clean up any update-related files in current directory
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let temp_download = exe_dir.join("temp_download");
            if temp_download.exists() {
                let _ = std::fs::remove_file(temp_download);
            }
        }
    }
}

/// Apply any pending updates and clean up old exe files.
pub fn apply_pending_updates() {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let staging_path = exe_dir.join("update_pending.exe");
            let backup_path = exe_path.with_extension("exe.old");

            // If there's a pending update, apply it
            if staging_path.exists() {
                // Backup current exe
                let _ = std::fs::copy(&exe_path, &backup_path);
                // Replace with staged exe
                if std::fs::rename(&staging_path, &exe_path).is_ok() {
                    // Success - cleanup temp file
                    let _ = std::fs::remove_file("temp_download");
                }
            }

            // Clean up old exe files
            let current_exe_name = exe_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if let Ok(entries) = std::fs::read_dir(exe_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();

                    // Delete old ScreenGoatedToolbox_v*.exe files (keep only current)
                    if (name_str.starts_with("ScreenGoatedToolbox_v") && name_str.ends_with(".exe"))
                        && name_str.as_ref() != current_exe_name
                    {
                        let _ = std::fs::remove_file(entry.path());
                    }

                    // Delete .old backup files
                    if name_str.ends_with(".exe.old") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

/// Set up crash handler to show message box on panic.
pub fn setup_crash_handler() {
    std::panic::set_hook(Box::new(|panic_info| {
        // 1. Format the error message
        let location = if let Some(location) = panic_info.location() {
            format!("File: {}\nLine: {}", location.file(), location.line())
        } else {
            "Unknown location".to_string()
        };

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };

        let error_msg = format!(
            "CRASH DETECTED!\n\nError: {}\n\nLocation:\n{}",
            payload, location
        );

        // Show a Windows Message Box so the user knows it crashed
        let wide_msg: Vec<u16> = error_msg.encode_utf16().chain(std::iter::once(0)).collect();
        let wide_title: Vec<u16> = "SGT Crash Report"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            MessageBoxW(
                None,
                PCWSTR(wide_msg.as_ptr()),
                PCWSTR(wide_title.as_ptr()),
                MB_ICONERROR | MB_OK,
            );
        }
    }));
}

/// Initialize COM and set DPI awareness.
pub fn init_com_and_dpi() {
    unsafe {
        use windows::Win32::System::Com::CoInitialize;
        let _ = CoInitialize(None);

        // Force Per-Monitor V2 DPI Awareness for correct screen metrics
        if let Ok(hidpi) = LoadLibraryW(w!("user32.dll")) {
            if let Some(set_context) = GetProcAddress(
                hidpi,
                PCSTR::from_raw("SetProcessDpiAwarenessContext\0".as_ptr()),
            ) {
                let func: extern "system" fn(isize) -> BOOL = std::mem::transmute(set_context);
                // -4 is DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
                let _ = func(-4);
            }
        }
    }
}

/// Spawn warmup thread for overlay components.
pub fn spawn_warmup_thread() {
    use crate::overlay;

    std::thread::spawn(|| {
        // 0. Warmup fonts first (download/cache for instant display)
        overlay::html_components::font_manager::warmup_fonts();

        // Helper: Wait for tray popup to close before proceeding
        let wait_for_popup_close = || {
            while overlay::tray_popup::is_popup_open() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        };

        // 1. Start warmups
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 1. Warmup tray popup (with is_warmup=true to avoid focus stealing)
        wait_for_popup_close();
        overlay::tray_popup::warmup_tray_popup();

        // 1.5 Warmup preset wheel (persistent hidden window)
        overlay::preset_wheel::warmup();

        // 2. Wait for splash screen / main box to appear and settle
        std::thread::sleep(std::time::Duration::from_millis(3000));

        // 3. Warmup text input window first (more likely to be used quickly)
        wait_for_popup_close();
        overlay::text_input::warmup();

        // 3.5 Warmup auto copy badge
        wait_for_popup_close();
        overlay::auto_copy_badge::warmup();

        // 3.75 Warmup text selection tag (native GDI)
        wait_for_popup_close();
        overlay::text_selection::warmup();

        // 7. Wait before realtime warmup
        std::thread::sleep(std::time::Duration::from_millis(5000));

        // 9. Warmup Recording Overlay
        wait_for_popup_close();
        overlay::recording::warmup_recording_overlay();
    });
}
