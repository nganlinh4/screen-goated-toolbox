// --- INITIALIZATION ---
// Application bootstrap: COM init, dark mode, cleanup, and warmups.

use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

#[cfg(windows)]
use std::io::ErrorKind;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use windows::Win32::System::Console::{SetConsoleCP, SetConsoleCtrlHandler, SetConsoleOutputCP};
#[cfg(windows)]
use windows::Win32::System::Threading::ExitProcess;
#[cfg(windows)]
use windows_core::BOOL;
#[cfg(windows)]
use winreg::RegKey;
#[cfg(windows)]
use winreg::enums::HKEY_CURRENT_USER;

#[cfg(windows)]
static CONSOLE_EXIT_STARTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub fn setup_console_utf8() -> bool {
    const CP_UTF8: u32 = 65001;
    unsafe {
        let input = SetConsoleCP(CP_UTF8);
        let output = SetConsoleOutputCP(CP_UTF8);
        input.is_ok() && output.is_ok()
    }
}

#[cfg(not(windows))]
pub fn setup_console_utf8() -> bool {
    true
}

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
    let bin_dir = crate::paths::app_local_data_dir().join("bin");

    if bin_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&bin_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "tmp") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    // 3. Clean up any update-related files in current directory
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let temp_download = exe_dir.join("temp_download");
        if temp_download.exists() {
            let _ = std::fs::remove_file(temp_download);
        }
    }
}

/// Apply any pending updates and clean up old exe files.
pub fn apply_pending_updates() {
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CrashReportMode {
    Interactive,
    NonInteractive,
}

impl CrashReportMode {
    fn shows_modal_dialog(self) -> bool {
        self == Self::Interactive
    }

    fn cleans_persisted_diagnostics(self) -> bool {
        self == Self::Interactive
    }
}

/// Set up crash reporting without blocking non-interactive processes on UI.
pub(crate) fn setup_crash_handler(mode: CrashReportMode) {
    if mode.cleans_persisted_diagnostics() {
        remove_persisted_crash_diagnostics();
    }
    setup_console_ctrl_handler();

    std::panic::set_hook(Box::new(move |panic_info| {
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

        // Release builds have no visible stderr. Persist the panic before showing
        // the modal so the next diagnostic pass has the actual failure text.
        crate::debug_log::log_debug(&format!("[Crash] {}", error_msg.replace('\n', " | ")));

        // Headless runners must always receive a report through their inherited
        // stderr without waiting for a desktop interaction.
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "{error_msg}");

        if !mode.shows_modal_dialog() {
            return;
        }

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

fn remove_persisted_crash_diagnostics() {
    remove_persisted_windows_error_reporting_dumps();

    let Some(local_data_dir) = dirs::data_local_dir() else {
        return;
    };
    let dump_dir = local_data_dir
        .join("screen-goated-toolbox")
        .join("crash-dumps");
    if !dump_dir.exists() {
        return;
    }

    match std::fs::remove_dir_all(&dump_dir) {
        Ok(()) => crate::log_info!(
            "[CrashDiag] removed persisted crash dump directory {}",
            dump_dir.display()
        ),
        Err(error) => crate::log_info!(
            "[CrashDiag] failed to remove persisted crash dump directory {}: {}",
            dump_dir.display(),
            error
        ),
    }
}

#[cfg(windows)]
fn setup_console_ctrl_handler() {
    unsafe {
        if SetConsoleCtrlHandler(Some(console_ctrl_handler), true).is_ok() {
            crate::log_info!("[Shutdown] console Ctrl handler enabled");
        } else {
            crate::log_info!("[Shutdown] console Ctrl handler unavailable");
        }
    }
}

#[cfg(not(windows))]
fn setup_console_ctrl_handler() {}

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    const CTRL_LOGOFF_EVENT: u32 = 5;
    const CTRL_SHUTDOWN_EVENT: u32 = 6;

    if !matches!(
        ctrl_type,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    ) {
        return BOOL(0);
    }

    if CONSOLE_EXIT_STARTED.swap(true, Ordering::SeqCst) {
        unsafe {
            ExitProcess(130);
        }
    }

    std::thread::spawn(move || {
        crate::log_info!(
            "[Shutdown] console control event {} received; using bounded hard-exit path",
            ctrl_type
        );
        std::thread::sleep(std::time::Duration::from_millis(250));
        unsafe {
            ExitProcess(130);
        }
    });

    BOOL(1)
}

#[cfg(windows)]
fn remove_persisted_windows_error_reporting_dumps() {
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "screen-goated-toolbox.exe".to_string());

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let local_dumps_path = format!(
        r"Software\Microsoft\Windows\Windows Error Reporting\LocalDumps\{}",
        exe_name
    );
    match hkcu.delete_subkey_all(&local_dumps_path) {
        Ok(()) => crate::log_info!("[CrashDiag] removed WER LocalDumps for {}", exe_name),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            crate::log_info!(
                "[CrashDiag] failed to remove WER LocalDumps for {}: {}",
                exe_name,
                error
            );
        }
    }
}

#[cfg(not(windows))]
fn remove_persisted_windows_error_reporting_dumps() {}

/// Initialize COM and set DPI awareness.
pub fn init_com_and_dpi() {
    unsafe {
        use windows::Win32::System::Com::CoInitialize;
        let _ = CoInitialize(None);

        // Force Per-Monitor V2 DPI Awareness for correct screen metrics
        if let Ok(hidpi) = LoadLibraryW(w!("user32.dll"))
            && let Some(set_context) = GetProcAddress(hidpi, s!("SetProcessDpiAwarenessContext"))
        {
            let func: extern "system" fn(isize) -> BOOL = std::mem::transmute(set_context);
            // -4 is DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
            let _ = func(-4);
        }
    }
}

/// Spawn warmup thread for overlay components.
pub fn spawn_warmup_thread() {
    // Startup warmups are intentionally disabled.
    // All overlays now initialize on first use.
}

#[cfg(test)]
mod crash_report_tests {
    use super::CrashReportMode;

    #[test]
    fn only_interactive_startup_may_touch_host_diagnostics_or_open_a_modal() {
        assert!(CrashReportMode::Interactive.shows_modal_dialog());
        assert!(!CrashReportMode::NonInteractive.shows_modal_dialog());
        assert!(CrashReportMode::Interactive.cleans_persisted_diagnostics());
        assert!(!CrashReportMode::NonInteractive.cleans_persisted_diagnostics());
    }
}
