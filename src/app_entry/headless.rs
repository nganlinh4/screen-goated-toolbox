use super::arguments::StartupArgs;
use std::fmt::Display;

const GT_NARRATION_TEST_FLAG: &str = "--gt-narration-test";
const COMPUTER_CONTROL_PROBE_FLAG: &str = "--computer-control-probe";
const COMPUTER_CONTROL_RUN_FLAG: &str = "--computer-control-run";
const COORD_TEST_FLAG: &str = "--cc-coord-test";
const UIA_DUMP_FLAG: &str = "--cc-uia-dump";
const VISION_TEST_FLAG: &str = "--cc-vision-test";
const DETECTOR_TEST_FLAG: &str = "--cc-detector-test";
const CURSOR_DEMO_FLAG: &str = "--cc-cursor-demo";
const GRID_TEST_FLAG: &str = "--cc-grid-test";
const UIA_TASK_FLAG: &str = "--cc-uia-task";
const MCP_TEST_FLAG: &str = "--cc-mcp-test";
const SYSTEM_QUERY_TEST_FLAG: &str = "--cc-system-query-test";
const TASK_TRACE_FLAG: &str = "--cc-task-trace";

const POST_UNPACK_MODE_FLAGS: &[&str] = &[
    GT_NARRATION_TEST_FLAG,
    COMPUTER_CONTROL_PROBE_FLAG,
    COMPUTER_CONTROL_RUN_FLAG,
    COORD_TEST_FLAG,
    UIA_DUMP_FLAG,
    VISION_TEST_FLAG,
    DETECTOR_TEST_FLAG,
    CURSOR_DEMO_FLAG,
    GRID_TEST_FLAG,
    UIA_TASK_FLAG,
    MCP_TEST_FLAG,
    SYSTEM_QUERY_TEST_FLAG,
    TASK_TRACE_FLAG,
];

pub(crate) fn is_requested(args: &StartupArgs) -> bool {
    args.has(crate::api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG)
        || POST_UNPACK_MODE_FLAGS.iter().any(|flag| args.has(flag))
        || super::replay::is_requested(args)
}

pub(crate) fn run_pre_boot(args: &StartupArgs) -> Option<i32> {
    args.has(crate::api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG)
        .then(crate::api::realtime_audio::sherpa_onnx::ffi_tts::run_load_probe_process)
}

pub(crate) fn run_post_unpack(args: &StartupArgs) -> Option<i32> {
    // Keep this dispatch order aligned with the historical entrypoint: when
    // callers pass multiple mode flags, the first matching mode owns the process.
    if let Some(input_wav) = args.value(GT_NARRATION_TEST_FLAG) {
        let target_language = args
            .value("--gt-narration-lang")
            .unwrap_or_else(|| "vi".to_string());
        return Some(report_result(
            crate::overlay::screen_record::run_gt_narration_test_cli(&input_wav, &target_language),
            "gt-test",
        ));
    }

    if args.has(COMPUTER_CONTROL_PROBE_FLAG) {
        let task = args.value("--cc-task").unwrap_or_else(|| {
            "Look at the screen and describe what you see, then call done.".to_string()
        });
        let tasks = match args.value("--cc-turns-json") {
            Some(raw) => match parse_scripted_turns(&raw) {
                Ok(tasks) => tasks,
                Err(ScriptedTurnsError::Empty) => {
                    eprintln!(
                        "[cc-probe] ERROR: --cc-turns-json must contain non-empty task strings"
                    );
                    return Some(2);
                }
                Err(ScriptedTurnsError::Invalid(error)) => {
                    eprintln!("[cc-probe] ERROR: invalid --cc-turns-json: {error}");
                    return Some(2);
                }
            },
            None => vec![task],
        };
        return Some(report_result(
            crate::overlay::computer_control::run_probe_cli(&tasks),
            "cc-probe",
        ));
    }

    if args.has(COMPUTER_CONTROL_RUN_FLAG) {
        let scripted_turns = match args.value("--cc-turns-json") {
            Some(raw) => match parse_scripted_turns(&raw) {
                Ok(turns) => Some(turns),
                Err(ScriptedTurnsError::Empty) => {
                    eprintln!("[cc-runtime] ERROR: scripted turns must be non-empty strings");
                    return Some(2);
                }
                Err(ScriptedTurnsError::Invalid(error)) => {
                    eprintln!("[cc-runtime] ERROR: invalid --cc-turns-json: {error}");
                    return Some(2);
                }
            },
            None => None,
        };
        return Some(report_result(
            crate::overlay::computer_control::run_headless(scripted_turns),
            "cc-runtime",
        ));
    }

    if args.has(COORD_TEST_FLAG) {
        return Some(report_result(
            crate::overlay::computer_control::run_coord_test_cli(),
            "coord",
        ));
    }

    if args.has(UIA_DUMP_FLAG) {
        let target = args
            .value("--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        return Some(report_result(
            crate::overlay::computer_control::run_uia_dump_cli(target.as_deref()),
            "uia",
        ));
    }

    if args.has(VISION_TEST_FLAG) {
        let target = args
            .value("--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        let question = args.value("--cc-task").unwrap_or_else(|| {
            "In one sentence, what application and content is shown?".to_string()
        });
        return Some(report_result(
            crate::overlay::computer_control::run_vision_test_cli(target.as_deref(), &question),
            "vision-test",
        ));
    }

    if args.has(DETECTOR_TEST_FLAG) {
        let target = args
            .value("--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        return Some(report_result(
            crate::overlay::computer_control::run_detector_test_cli(target.as_deref()),
            "detector-test",
        ));
    }

    if args.has(CURSOR_DEMO_FLAG) {
        crate::overlay::computer_control::run_cursor_demo_cli();
        return Some(0);
    }

    if args.has(GRID_TEST_FLAG) {
        let target = args
            .value("--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        return Some(report_result(
            crate::overlay::computer_control::run_grid_test_cli(target.as_deref()),
            "grid-test",
        ));
    }

    if args.has(UIA_TASK_FLAG) {
        let task = args
            .value("--cc-task")
            .unwrap_or_else(|| "Describe the focused window, then call done.".to_string());
        return Some(report_result(
            crate::overlay::computer_control::run_uia_task_cli(&task),
            "uia-task",
        ));
    }

    if args.has(MCP_TEST_FLAG) {
        let id = args
            .value(MCP_TEST_FLAG)
            .unwrap_or_else(|| "time".to_string());
        let tool = args.value("--cc-mcp-tool");
        let args_json = args.value("--cc-mcp-args-json");
        let list_only = args.has("--cc-mcp-list-only");
        return Some(report_result(
            crate::overlay::computer_control::run_mcp_test_cli(
                &id,
                tool.as_deref(),
                args_json.as_deref(),
                list_only,
            ),
            "mcp-test",
        ));
    }

    if args.has(SYSTEM_QUERY_TEST_FLAG) {
        let spec = args
            .value(SYSTEM_QUERY_TEST_FLAG)
            .unwrap_or_else(|| "capabilities.list".to_string());
        let args_json = args.value("--cc-system-query-args-json");
        return Some(report_result(
            crate::overlay::computer_control::run_system_query_test_cli(
                &spec,
                args_json.as_deref(),
            ),
            "system-query-test",
        ));
    }

    if args.has(TASK_TRACE_FLAG) {
        let task = args
            .value("--cc-task")
            .unwrap_or_else(|| "Open the Windows Start menu, then call done.".to_string());
        return Some(report_result(
            crate::overlay::computer_control::run_task_trace_cli(&task),
            "trace",
        ));
    }

    super::replay::run(args)
}

fn report_result<E: Display>(result: Result<(), E>, label: &str) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("[{label}] ERROR: {error}");
            1
        }
    }
}

#[derive(Debug)]
enum ScriptedTurnsError {
    Empty,
    Invalid(serde_json::Error),
}

fn parse_scripted_turns(raw: &str) -> Result<Vec<String>, ScriptedTurnsError> {
    let turns = serde_json::from_str::<Vec<String>>(raw).map_err(ScriptedTurnsError::Invalid)?;
    if turns.is_empty() || turns.iter().any(|turn| turn.trim().is_empty()) {
        return Err(ScriptedTurnsError::Empty);
    }
    Ok(turns)
}

#[cfg(test)]
mod tests {
    use super::{
        POST_UNPACK_MODE_FLAGS, ScriptedTurnsError, StartupArgs, is_requested, parse_scripted_turns,
    };

    fn args(raw: &[&str]) -> StartupArgs {
        let mut full = vec!["sgt.exe"];
        full.extend_from_slice(raw);
        StartupArgs::for_test(&full)
    }

    #[test]
    fn headless_detection_covers_every_dispatch_mode_without_value_inference() {
        for flag in POST_UNPACK_MODE_FLAGS {
            assert!(is_requested(&args(&[flag])), "flag={flag}");
        }
        assert!(is_requested(&args(&[
            crate::api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG,
        ])));
        assert!(is_requested(&args(&[
            super::super::replay::EXPORT_REPLAY_FLAG
        ])));
        assert!(is_requested(&args(&[
            super::super::replay::EXPORT_REPLAY_LAST_FLAG,
        ])));

        assert!(!is_requested(&args(&[])));
        assert!(!is_requested(&args(&["--cc-task", "descriptive text"])));
    }

    #[test]
    fn scripted_turns_require_a_non_empty_list_of_non_empty_strings() {
        assert_eq!(
            parse_scripted_turns(r#"["first", " second "]"#).unwrap(),
            ["first", " second "]
        );

        for raw in ["[]", r#"[""]"#, r#"["valid", "   "]"#] {
            assert!(matches!(
                parse_scripted_turns(raw),
                Err(ScriptedTurnsError::Empty)
            ));
        }
    }

    #[test]
    fn scripted_turns_reject_invalid_json_or_non_string_values() {
        for raw in ["not-json", r#"["valid", 3]"#, r#"{"turn":"valid"}"#] {
            assert!(matches!(
                parse_scripted_turns(raw),
                Err(ScriptedTurnsError::Invalid(_))
            ));
        }
    }
}
