//! Process a requested WebView reset before any WebView consumer starts.
use std::io;

pub(super) fn clear_pending_webview(owns_activation: bool) -> io::Result<()> {
    let pending = crate::APP
        .lock()
        .map_err(|_| io::Error::other("application state is unavailable"))?
        .config
        .clear_webview_on_startup;
    process_pending(
        pending,
        owns_activation,
        crate::overlay::webview_reset::clear_webview_permissions,
        || {
            crate::APP
                .lock()
                .map_err(|_| io::Error::other("application state is unavailable"))?
                .config
                .clear_webview_on_startup = false;
            if let Err(error) = crate::config::try_save_current_config() {
                crate::APP
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .config
                    .clear_webview_on_startup = true;
                return Err(error);
            }
            Ok(())
        },
    )
}

fn process_pending(
    pending: bool,
    owns_activation: bool,
    clear: impl FnOnce() -> io::Result<()>,
    save: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if !pending {
        return Ok(());
    }
    if !owns_activation {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "WebView reset requires the owning application instance",
        ));
    }
    clear()?;
    save()
}

pub(super) fn report_failure() {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        use windows::core::w;
        MessageBoxW(
            None,
            w!(
                "WebView data could not be cleared. The reset remains pending. Close other running copies and remove any custom WebView data-directory override before trying again."
            ),
            w!("Screen Goated Toolbox"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(test)]
#[path = "reset_webview_tests.rs"]
mod tests;
