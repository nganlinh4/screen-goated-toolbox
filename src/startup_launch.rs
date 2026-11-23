use std::time::Duration;

pub(crate) const AUTOSTART_ARG: &str = "--sgt-autostart";

const STARTUP_STABILIZATION_DELAY_SECS: u64 = 20;
const EARLY_BOOT_GRACE_SECS: u64 = 180;

pub(crate) fn maybe_delay_for_windows_autostart(args: &[std::ffi::OsString]) {
    let uptime = windows_uptime_secs();
    if let Some(reason) = autostart_reason(args, uptime, current_exe_registered_for_startup) {
        let uptime = uptime
            .map(|secs| secs.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        crate::log_info!(
            "[Startup] Windows autostart detected; delaying GUI initialization by {}s (uptime={}s, explicit={})",
            STARTUP_STABILIZATION_DELAY_SECS,
            uptime,
            reason == Autostart::Explicit
        );
        std::thread::sleep(Duration::from_secs(STARTUP_STABILIZATION_DELAY_SECS));
    }
}

#[derive(Debug, PartialEq)]
enum Autostart {
    Explicit,
    Legacy,
}

fn autostart_reason(
    args: &[std::ffi::OsString],
    uptime: Option<u64>,
    registered: impl FnOnce() -> bool,
) -> Option<Autostart> {
    let (_, arguments) = args.split_first()?;
    if arguments.iter().any(|arg| arg != AUTOSTART_ARG) {
        return None;
    }
    if !arguments.is_empty() {
        return Some(Autostart::Explicit);
    }
    if uptime.is_some_and(|uptime| uptime < EARLY_BOOT_GRACE_SECS) && registered() {
        Some(Autostart::Legacy)
    } else {
        None
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
    if crate::paths::runtime_state_root().is_some() {
        return false;
    }
    let Ok(exe_path) = std::env::current_exe() else {
        return false;
    };
    crate::admin_startup::provider::CurrentUserStartup::new(&exe_path, "")
        .is_enabled()
        .unwrap_or(false)
        || crate::admin_startup::enabled_for(&exe_path)
}

#[cfg(not(windows))]
fn current_exe_registered_for_startup() -> bool {
    false
}

#[cfg(test)]
mod tests;
