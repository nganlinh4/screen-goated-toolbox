use std::time::Duration;

pub(crate) const AUTOSTART_ARG: &str = "--sgt-autostart";

const STARTUP_STABILIZATION_DELAY_SECS: u64 = 20;
const EARLY_BOOT_GRACE_SECS: u64 = 180;

pub(crate) fn maybe_delay_for_windows_autostart(args: &[String]) {
    if args.iter().skip(1).any(|arg| arg != AUTOSTART_ARG) {
        return;
    }

    let explicit_autostart = args.iter().any(|arg| arg == AUTOSTART_ARG);
    let inferred_legacy_autostart = !explicit_autostart
        && current_exe_registered_for_startup()
        && windows_uptime_secs().is_some_and(|uptime| uptime < EARLY_BOOT_GRACE_SECS);

    if explicit_autostart || inferred_legacy_autostart {
        let uptime = windows_uptime_secs()
            .map(|secs| secs.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        crate::log_info!(
            "[Startup] Windows autostart detected; delaying GUI initialization by {}s (uptime={}s, explicit={})",
            STARTUP_STABILIZATION_DELAY_SECS,
            uptime,
            explicit_autostart
        );
        std::thread::sleep(Duration::from_secs(STARTUP_STABILIZATION_DELAY_SECS));
    }
}

#[cfg(windows)]
fn windows_uptime_secs() -> Option<u64> {
    Some(unsafe { windows::Win32::System::SystemInformation::GetTickCount64() / 1_000 })
}

#[cfg(not(windows))]
fn windows_uptime_secs() -> Option<u64> {
    None
}

#[cfg(windows)]
fn current_exe_registered_for_startup() -> bool {
    let Ok(exe_path) = std::env::current_exe() else {
        return false;
    };
    let exe = exe_path.to_string_lossy().to_lowercase();

    registry_run_entry_points_to(&exe) || admin_task_points_to(&exe)
}

#[cfg(not(windows))]
fn current_exe_registered_for_startup() -> bool {
    false
}

#[cfg(windows)]
fn registry_run_entry_points_to(exe: &str) -> bool {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        KEY_READ,
    ) else {
        return false;
    };

    key.get_value::<String, &str>("ScreenGoatedToolbox")
        .map(|value| value.to_lowercase().contains(exe))
        .unwrap_or(false)
}

#[cfg(windows)]
fn admin_task_points_to(exe: &str) -> bool {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = std::process::Command::new("schtasks")
        .args(["/query", "/tn", "ScreenGoatedToolbox_AutoStart", "/xml"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_lowercase())
        .is_some_and(|xml| xml.contains(exe))
}
