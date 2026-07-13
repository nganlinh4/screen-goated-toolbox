use std::path::{Path, PathBuf};

pub(super) const PROCESS_WITH_SGT_FLAG: &str = "--process-with-sgt";
const SCREEN_RECORD_WRY_SMOKE_FLAG: &str = "--screen-record-wry-smoke";
const SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--screen-record-webview2-debug-port";

pub(crate) struct StartupArgs {
    raw: Vec<String>,
}

impl StartupArgs {
    pub(crate) fn collect() -> Self {
        Self {
            raw: std::env::args().collect(),
        }
    }

    pub(crate) fn raw(&self) -> &[String] {
        &self.raw
    }

    pub(crate) fn has(&self, key: &str) -> bool {
        self.raw.iter().any(|arg| arg == key)
    }

    pub(crate) fn value(&self, key: &str) -> Option<String> {
        self.raw
            .iter()
            .position(|arg| arg == key)
            .and_then(|index| self.raw.get(index + 1))
            .cloned()
    }

    pub(crate) fn process_with_sgt_file(&self) -> Option<PathBuf> {
        find_process_with_sgt_file(&self.raw, |path| path.exists() && path.is_file())
    }

    pub(crate) fn configure_screen_record_wry_smoke(&self) -> bool {
        let smoke_enabled = self.has(SCREEN_RECORD_WRY_SMOKE_FLAG);
        let Some(port) = self.value(SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG) else {
            return smoke_enabled;
        };
        if !is_valid_webview2_debug_port(&port) {
            crate::log_info!("[WrySmoke] Ignoring invalid WebView2 debug port: {port}");
            return smoke_enabled;
        }

        let remote_arg =
            format!("--remote-debugging-port={port} --remote-debugging-address=0.0.0.0");
        let next_args = match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
            Ok(existing) if !existing.trim().is_empty() => format!("{existing} {remote_arg}"),
            _ => remote_arg,
        };
        unsafe {
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", next_args);
            if std::env::var("SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR").is_err() {
                std::env::set_var(
                    "SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR",
                    std::env::temp_dir()
                        .join(format!("sgt-record-wry-smoke-webview2-{port}"))
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
        crate::log_info!("[WrySmoke] Enabled WebView2 remote debugging on port {port}");
        smoke_enabled
    }
}

fn find_process_with_sgt_file(
    args: &[String],
    mut is_file: impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    if !args.iter().any(|arg| arg == PROCESS_WITH_SGT_FLAG) {
        return None;
    }

    args.iter().skip(1).find_map(|arg| {
        if arg.starts_with("--") {
            return None;
        }
        let path = PathBuf::from(arg);
        is_file(&path).then_some(path)
    })
}

fn is_valid_webview2_debug_port(port: &str) -> bool {
    port.parse::<u16>().ok().is_some_and(|value| value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> StartupArgs {
        StartupArgs {
            raw: raw.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }

    #[test]
    fn argument_lookup_preserves_first_match_and_missing_value_semantics() {
        let startup_args = args(&[
            "sgt.exe",
            "--model",
            "first",
            "--flag",
            "--model",
            "second",
            "--trailing",
        ]);

        assert!(startup_args.has("--flag"));
        assert!(!startup_args.has("--missing"));
        assert_eq!(startup_args.value("--model").as_deref(), Some("first"));
        assert_eq!(startup_args.value("--flag").as_deref(), Some("--model"));
        assert_eq!(startup_args.value("--trailing"), None);
        assert_eq!(startup_args.value("--missing"), None);
    }

    #[test]
    fn process_file_selection_requires_flag_and_skips_flag_shaped_arguments() {
        let without_process_flag = args(&["sgt.exe", "candidate.txt"]);
        assert_eq!(
            find_process_with_sgt_file(without_process_flag.raw(), |_| true),
            None
        );

        let startup_args = args(&[
            "sgt.exe",
            PROCESS_WITH_SGT_FLAG,
            "--ignored",
            "missing.txt",
            "selected.txt",
        ]);
        let selected = find_process_with_sgt_file(startup_args.raw(), |path| {
            path == Path::new("selected.txt")
        });

        assert_eq!(selected, Some(PathBuf::from("selected.txt")));
    }

    #[test]
    fn webview2_debug_port_validation_rejects_invalid_or_zero_values() {
        for port in ["", "0", "-1", "65536", "not-a-port", "12.5"] {
            assert!(!is_valid_webview2_debug_port(port), "port={port}");
        }
        for port in ["1", "9222", "65535", "0001"] {
            assert!(is_valid_webview2_debug_port(port), "port={port}");
        }
    }
}
