use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

fn ui_test_temp_root() -> PathBuf {
    if crate::paths::runtime_state_root().is_some() {
        crate::paths::app_temp_dir()
    } else {
        std::env::temp_dir()
    }
}

pub(super) const PROCESS_WITH_SGT_FLAG: &str = "--process-with-sgt";
const SCREEN_RECORD_WRY_SMOKE_FLAG: &str = "--screen-record-wry-smoke";
const SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--screen-record-webview2-debug-port";
const CREATION_UI_TEST_FLAG: &str = "--creation-ui-test";
const CREATION_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--creation-webview2-debug-port";
const SCREEN_TRANSLATE_UI_TEST_FLAG: &str = "--screen-translate-ui-test";
const SCREEN_TRANSLATE_LAB_QUEUE_FLAG: &str = "--screen-translate-lab-queue";
const RESULT_COMPOSITOR_SMOKE_FLAG: &str = "--result-compositor-smoke";
const STATUS_COMPOSITOR_SMOKE_FLAG: &str = "--status-compositor-smoke";
const REALTIME_COMPOSITOR_SMOKE_FLAG: &str = "--realtime-compositor-smoke";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationUiTestApp {
    ThreeD,
    Svg,
}

pub(crate) struct StartupArgs {
    raw: Vec<OsString>,
}

impl StartupArgs {
    pub(crate) fn collect() -> Self {
        Self {
            raw: std::env::args_os().collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(raw: &[&str]) -> Self {
        Self {
            raw: raw.iter().map(|arg| OsString::from(*arg)).collect(),
        }
    }

    pub(crate) fn raw(&self) -> &[OsString] {
        &self.raw
    }

    pub(crate) fn has(&self, key: &str) -> bool {
        self.raw.iter().any(|arg| arg == key)
    }

    pub(crate) fn value(&self, key: &str) -> Option<String> {
        self.value_os(key)?.to_str().map(str::to_owned)
    }

    fn value_os(&self, key: &str) -> Option<&OsStr> {
        self.raw
            .iter()
            .position(|arg| arg == key)
            .and_then(|index| self.raw.get(index + 1))
            .map(OsString::as_os_str)
    }

    /// The requested file, resolved against this process's working directory:
    /// a secondary launch hands it to the running instance, whose directory differs.
    pub(crate) fn process_with_sgt_file(&self) -> Option<PathBuf> {
        find_process_with_sgt_file(&self.raw, |path| path.exists() && path.is_file())
            .and_then(|path| std::path::absolute(path).ok())
    }

    pub(crate) fn validated_process_with_sgt_file(&self) -> std::io::Result<Option<PathBuf>> {
        let path = self.process_with_sgt_file();
        if self.has(PROCESS_WITH_SGT_FLAG) && path.is_none() {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the requested file is missing or inaccessible",
            ))
        } else {
            Ok(path)
        }
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
                    ui_test_temp_root()
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

        unsafe {
            std::env::set_var("SGT_CREATION_WEBVIEW2_DEBUG_PORT", &port);
            if std::env::var("SGT_CREATION_WEBVIEW2_DATA_DIR").is_err() {
                std::env::set_var(
                    "SGT_CREATION_WEBVIEW2_DATA_DIR",
                    ui_test_temp_root()
                        .join(format!("sgt-creation-ui-test-webview2-{port}"))
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
        crate::log_info!("[CreationUiTest] Enabled WebView2 remote debugging on port {port}");
        Some(app)
    }

    pub(crate) fn screen_translate_ui_test(&self) -> bool {
        self.has(SCREEN_TRANSLATE_UI_TEST_FLAG)
    }

    pub(crate) fn screen_translate_ui_test_image(&self) -> Option<PathBuf> {
        self.value_os(SCREEN_TRANSLATE_UI_TEST_FLAG)
            .filter(|value| !is_option(value))
            .map(PathBuf::from)
            .filter(|path| path.is_file())
    }

    pub(crate) fn screen_translate_lab_queue(&self) -> Option<PathBuf> {
        self.value_os(SCREEN_TRANSLATE_LAB_QUEUE_FLAG)
            .filter(|value| !is_option(value))
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_dir())
    }

    pub(crate) fn result_compositor_smoke(&self) -> bool {
        self.has(RESULT_COMPOSITOR_SMOKE_FLAG)
    }

    pub(crate) fn status_compositor_smoke(&self) -> bool {
        self.has(STATUS_COMPOSITOR_SMOKE_FLAG)
    }

    pub(crate) fn realtime_compositor_smoke(&self) -> bool {
        self.has(REALTIME_COMPOSITOR_SMOKE_FLAG)
    }
}

fn find_process_with_sgt_file(
    args: &[OsString],
    mut is_file: impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    let index = args.iter().position(|arg| arg == PROCESS_WITH_SGT_FLAG)?;
    let value = args.get(index + 1)?;
    if is_known_option(value) {
        return None;
    }
    let path = PathBuf::from(value);
    is_file(&path).then_some(path)
}

fn is_known_option(value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    matches!(
        value,
        PROCESS_WITH_SGT_FLAG
            | SCREEN_RECORD_WRY_SMOKE_FLAG
            | SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG
            | CREATION_UI_TEST_FLAG
            | CREATION_WEBVIEW2_DEBUG_PORT_FLAG
            | SCREEN_TRANSLATE_UI_TEST_FLAG
            | SCREEN_TRANSLATE_LAB_QUEUE_FLAG
            | RESULT_COMPOSITOR_SMOKE_FLAG
            | STATUS_COMPOSITOR_SMOKE_FLAG
            | REALTIME_COMPOSITOR_SMOKE_FLAG
            | "--restarted"
            | "--sgt-autostart"
            | "--cc-coord-test"
            | "--cc-cursor-demo"
            | "--cc-grid-test"
            | "--cc-mcp-test"
            | "--cc-system-query-test"
            | "--cc-task-trace"
            | "--cc-uia-dump"
            | "--cc-uia-task"
            | "--cc-vision-test"
            | "--computer-control-probe"
            | "--computer-control-run"
            | "--gt-narration-lang"
            | "--gt-narration-test"
            | "--sr-export-replay-bench"
            | "--sr-export-replay-keep-output"
            | "--sr-export-replay-last"
            | "--sr-export-replay"
    )
}

fn is_option(arg: &OsStr) -> bool {
    arg.as_encoded_bytes().starts_with(b"--")
}

fn is_valid_webview2_debug_port(port: &str) -> bool {
    port.parse::<u16>().ok().is_some_and(|value| value > 0)
}

fn parse_creation_ui_test_app(value: Option<&str>) -> Option<CreationUiTestApp> {
    match value {
        Some("3d") => Some(CreationUiTestApp::ThreeD),
        Some("svg") => Some(CreationUiTestApp::Svg),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> StartupArgs {
        StartupArgs::for_test(raw)
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
    fn process_file_selection_binds_only_the_flag_value() {
        let without_process_flag = args(&["sgt.exe", "candidate.txt"]);
        assert_eq!(
            find_process_with_sgt_file(without_process_flag.raw(), |_| true),
            None
        );

        let startup_args = args(&[
            "sgt.exe",
            "--other",
            "other.txt",
            PROCESS_WITH_SGT_FLAG,
            "selected.txt",
        ]);
        assert_eq!(
            find_process_with_sgt_file(startup_args.raw(), |_| true),
            Some(PathBuf::from("selected.txt"))
        );
        assert!(
            find_process_with_sgt_file(startup_args.raw(), |path| path == Path::new("other.txt"))
                .is_none()
        );
        let missing = args(&[
            "sgt.exe",
            "--other",
            "other.txt",
            PROCESS_WITH_SGT_FLAG,
            SCREEN_RECORD_WRY_SMOKE_FLAG,
            "other.txt",
        ]);
        assert!(find_process_with_sgt_file(missing.raw(), |_| true).is_none());
        let leading_dash = args(&["sgt.exe", PROCESS_WITH_SGT_FLAG, "--report.txt"]);
        assert_eq!(
            find_process_with_sgt_file(leading_dash.raw(), |path| path
                == Path::new("--report.txt")),
            Some(PathBuf::from("--report.txt"))
        );
    }

    #[test]
    fn missing_file_request_is_not_a_restore_only_request() {
        let startup_args = args(&["sgt.exe", PROCESS_WITH_SGT_FLAG]);
        assert_eq!(
            startup_args
                .validated_process_with_sgt_file()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(
            args(&["sgt.exe"])
                .validated_process_with_sgt_file()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn relative_process_file_is_resolved_against_the_requesting_directory() {
        // A secondary launch forwards this path to the running instance, whose
        // working directory differs, so it must name the file absolutely.
        let relative = Path::new(file!());
        assert!(relative.is_relative() && relative.is_file());
        let startup_args = StartupArgs {
            raw: vec![
                OsString::from("sgt.exe"),
                OsString::from(PROCESS_WITH_SGT_FLAG),
                relative.as_os_str().to_os_string(),
            ],
        };
        let selected = startup_args.validated_process_with_sgt_file().unwrap();
        assert_eq!(
            selected,
            Some(std::env::current_dir().unwrap().join(relative))
        );
    }

    #[cfg(windows)]
    #[test]
    fn process_file_preserves_unpaired_utf16_surrogate() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        let file_name = OsString::from_wide(&[
            b's' as u16,
            b'g' as u16,
            b't' as u16,
            b'-' as u16,
            0xd800,
            b'.' as u16,
            b't' as u16,
            b'x' as u16,
            b't' as u16,
        ]);
        let directory =
            std::env::temp_dir().join(format!("sgt-argument-utf16-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join(file_name);
        std::fs::write(&path, b"test").unwrap();
        let startup_args = StartupArgs {
            raw: vec![
                OsString::from("sgt.exe"),
                OsString::from(PROCESS_WITH_SGT_FLAG),
                path.as_os_str().to_os_string(),
                OsString::from_wide(&[b'-' as u16, b'-' as u16, 0xd800]),
                OsString::from("--model"),
                OsString::from("first"),
            ],
        };
        let selected = startup_args.process_with_sgt_file().unwrap();
        assert_eq!(
            selected.as_os_str().encode_wide().collect::<Vec<_>>(),
            path.as_os_str().encode_wide().collect::<Vec<_>>()
        );
        assert!(startup_args.has(PROCESS_WITH_SGT_FLAG));
        assert_eq!(startup_args.value("--model").as_deref(), Some("first"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
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
        assert_eq!(
            parse_creation_ui_test_app(Some("svg")),
            Some(CreationUiTestApp::Svg)
        );
        for value in [None, Some(""), Some("3D"), Some("unknown")] {
            assert_eq!(parse_creation_ui_test_app(value), None);
        }
    }

    #[test]
    fn compositor_smoke_flags_are_isolated_ui_tests() {
        let result = args(&["sgt.exe", RESULT_COMPOSITOR_SMOKE_FLAG]);
        let status = args(&["sgt.exe", STATUS_COMPOSITOR_SMOKE_FLAG]);
        let realtime = args(&["sgt.exe", REALTIME_COMPOSITOR_SMOKE_FLAG]);
        assert!(result.result_compositor_smoke());
        assert!(status.status_compositor_smoke());
        assert!(realtime.realtime_compositor_smoke());
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

    #[test]
    fn screen_translate_lab_queue_requires_an_absolute_directory() {
        let directory = std::env::temp_dir();
        assert_eq!(
            args(&[
                "sgt.exe",
                SCREEN_TRANSLATE_LAB_QUEUE_FLAG,
                directory.to_str().unwrap()
            ])
            .screen_translate_lab_queue(),
            Some(directory)
        );
        assert_eq!(
            args(&["sgt.exe", SCREEN_TRANSLATE_LAB_QUEUE_FLAG, "relative"])
                .screen_translate_lab_queue(),
            None
        );
    }
}
