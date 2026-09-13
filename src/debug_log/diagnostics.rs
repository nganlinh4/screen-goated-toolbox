//! Explicitly enabled high-volume diagnostics; normal support events stay on.
use std::sync::LazyLock;

pub(crate) fn verbose_enabled() -> bool {
    static ENABLED: LazyLock<bool> =
        LazyLock::new(|| std::env::var("SGT_VERBOSE_LOGS").as_deref() == Ok("1"));
    *ENABLED
}

pub(crate) fn paste_enabled() -> bool {
    static TEXT: LazyLock<bool> =
        LazyLock::new(|| std::env::var("SGT_AUTOPASTE_TEXT_DIAGNOSTICS").as_deref() == Ok("1"));
    verbose_enabled() || *TEXT
}

pub(crate) fn health_interval() -> std::time::Duration {
    std::time::Duration::from_secs(if verbose_enabled() { 5 } else { 60 })
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {{
        if $crate::debug_log::diagnostics::verbose_enabled() {
            $crate::log_info!($($arg)*);
        }
    }};
}

#[macro_export]
macro_rules! log_paste_trace {
    ($($arg:tt)*) => {{
        if $crate::debug_log::diagnostics::paste_enabled() {
            $crate::log_info!($($arg)*);
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_flags_control_lazy_diagnostic_formatting() {
        let verbose = std::env::var("SGT_VERBOSE_LOGS").as_deref() == Ok("1");
        let text = std::env::var("SGT_AUTOPASTE_TEXT_DIAGNOSTICS").as_deref() == Ok("1");
        let mut trace_formatted = false;
        let mut paste_formatted = false;
        crate::log_trace!("{}", {
            trace_formatted = true;
            "trace metadata"
        });
        crate::log_paste_trace!("{}", {
            paste_formatted = true;
            "paste metadata"
        });
        assert_eq!(trace_formatted, verbose);
        assert_eq!(paste_formatted, verbose || text);
        assert_eq!(health_interval().as_secs(), if verbose { 5 } else { 60 });
    }
}
