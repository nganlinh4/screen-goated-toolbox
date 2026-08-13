use std::path::{Path, PathBuf};

pub(super) const PROCESS_WITH_SGT_FLAG: &str = "--process-with-sgt";
const SCREEN_RECORD_WRY_SMOKE_FLAG: &str = "--screen-record-wry-smoke";
const SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--screen-record-webview2-debug-port";
const CREATION_UI_TEST_FLAG: &str = "--creation-ui-test";
const CREATION_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--creation-webview2-debug-port";
const SCREEN_TRANSLATE_UI_TEST_FLAG: &str = "--screen-translate-ui-test";
const RESULT_COMPOSITOR_SMOKE_FLAG: &str = "--result-compositor-smoke";
const STATUS_COMPOSITOR_SMOKE_FLAG: &str = "--status-compositor-smoke";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationUiTestApp {
    ThreeD,
}

pub(crate) struct StartupArgs {
    raw: Vec<String>,
}

impl StartupArgs {
    pub(crate) fn collect() -> Self {
        Self {
            raw: std::env::args().collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(raw: &[&str]) -> Self {
        Self {
            raw: raw.iter().map(|arg| (*arg).to_string()).collect(),
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

        unsafe {
            std::env::set_var("SGT_RECORDER_WEBVIEW2_DEBUG_PORT", &port);
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

    pub(crate) fn configure_creation_ui_test(&self) -> Option<CreationUiTestApp> {
        let app = parse_creation_ui_test_app(self.value(CREATION_UI_TEST_FLAG).as_deref())?;
        let Some(port) = self.value(CREATION_WEBVIEW2_DEBUG_PORT_FLAG) else {
            crate::log_info!("[CreationUiTest] Missing WebView2 debug port");
            return None;
        };
        if !is_valid_webview2_debug_port(&port) {
            crate::log_info!("[CreationUiTest] Ignoring invalid WebView2 debug port: {port}");
            return None;
        }

        let remote_arg =
            format!("--remote-debugging-port={port} --remote-debugging-address=127.0.0.1");
        let next_args = match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
            Ok(existing) if !existing.trim().is_empty() => format!("{existing} {remote_arg}"),
            _ => remote_arg,
        };
        unsafe {
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", next_args);
            if std::env::var("SGT_CREATION_WEBVIEW2_DATA_DIR").is_err() {
                std::env::set_var(
                    "SGT_CREATION_WEBVIEW2_DATA_DIR",
                    std::env::temp_dir()
                        .join(format!("sgt-creation-ui-test-webview2-{port}"))
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
        crate::log_info!("[CreationUiTest] Enabled WebView2 remote debugging on port {port}");
        Some(app)
    }

    pub(crate) fn result_compositor_smoke(&self) -> bool {
        self.has(RESULT_COMPOSITOR_SMOKE_FLAG)
    }

    pub(crate) fn screen_translate_ui_test(&self) -> bool {
        self.has(SCREEN_TRANSLATE_UI_TEST_FLAG)
    }

    pub(crate) fn screen_translate_ui_test_image(&self) -> Option<PathBuf> {
        self.value(SCREEN_TRANSLATE_UI_TEST_FLAG)
            .filter(|value| !value.starts_with("--"))
            .map(PathBuf::from)
            .filter(|path| path.is_file())
    }

    pub(crate) fn status_compositor_smoke(&self) -> bool {
        self.has(STATUS_COMPOSITOR_SMOKE_FLAG)
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

fn parse_creation_ui_test_app(value: Option<&str>) -> Option<CreationUiTestApp> {
    match value {
        Some("3d") => Some(CreationUiTestApp::ThreeD),
        _ => None,
    }
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

    #[test]
    fn creation_ui_test_app_accepts_only_supported_public_apps() {
        assert_eq!(
            parse_creation_ui_test_app(Some("3d")),
            Some(CreationUiTestApp::ThreeD)
        );
        for value in [
            None,
            Some(""),
            Some("3D"),
            Some("image"),
            Some("svg"),
            Some("unknown"),
        ] {
            assert_eq!(parse_creation_ui_test_app(value), None);
        }
    }

    #[test]
    fn compositor_smoke_flags_are_isolated_ui_tests() {
        let result = args(&["sgt.exe", RESULT_COMPOSITOR_SMOKE_FLAG]);
        let status = args(&["sgt.exe", STATUS_COMPOSITOR_SMOKE_FLAG]);
        assert!(result.result_compositor_smoke());
        assert!(status.status_compositor_smoke());
    }

    #[test]
    fn screen_translate_ui_test_is_explicit() {
        assert!(args(&["sgt.exe", SCREEN_TRANSLATE_UI_TEST_FLAG]).screen_translate_ui_test());
        assert!(!args(&["sgt.exe"]).screen_translate_ui_test());
        assert_eq!(
            args(&["sgt.exe", SCREEN_TRANSLATE_UI_TEST_FLAG, file!()])
                .screen_translate_ui_test_image(),
            Some(PathBuf::from(file!()))
        );
        assert_eq!(
            args(&[
                "sgt.exe",
                SCREEN_TRANSLATE_UI_TEST_FLAG,
                RESULT_COMPOSITOR_SMOKE_FLAG
            ])
            .screen_translate_ui_test_image(),
            None
        );
    }
}
