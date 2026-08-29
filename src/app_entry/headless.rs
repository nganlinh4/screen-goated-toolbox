use super::arguments::StartupArgs;
use std::fmt::Display;

const GT_NARRATION_TEST_FLAG: &str = "--gt-narration-test";

pub(crate) fn is_requested(args: &StartupArgs) -> bool {
    args.has(crate::api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG)
        || args.has(GT_NARRATION_TEST_FLAG)
        || super::replay::is_requested(args)
}

pub(crate) fn run_pre_boot(args: &StartupArgs) -> Option<i32> {
    args.has(crate::api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG)
        .then(crate::api::realtime_audio::sherpa_onnx::ffi_tts::run_load_probe_process)
}

pub(crate) fn run_after_bootstrap(args: &StartupArgs) -> Option<i32> {
    if let Some(input_wav) = args.value(GT_NARRATION_TEST_FLAG) {
        let target_language = args
            .value("--gt-narration-lang")
            .unwrap_or_else(|| "vi".to_string());
        return Some(report_result(
            crate::overlay::screen_record::run_gt_narration_test_cli(&input_wav, &target_language),
            "gt-test",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> StartupArgs {
        let mut full = vec!["sgt.exe"];
        full.extend_from_slice(raw);
        StartupArgs::for_test(&full)
    }

    #[test]
    fn computer_control_flags_are_not_headless_entrances() {
        for flag in [
            "--computer-control-probe",
            "--computer-control-run",
            "--cc-coord-test",
            "--cc-uia-dump",
            "--cc-vision-test",
            "--cc-cursor-demo",
            "--cc-grid-test",
            "--cc-uia-task",
            "--cc-mcp-test",
            "--cc-system-query-test",
            "--cc-task-trace",
        ] {
            assert!(!is_requested(&args(&[flag])), "flag={flag}");
        }
    }

    #[test]
    fn unrelated_headless_modes_remain_available() {
        assert!(is_requested(&args(&[GT_NARRATION_TEST_FLAG, "input.wav"])));
        assert!(is_requested(&args(&[
            super::super::replay::EXPORT_REPLAY_FLAG,
            "replay.json",
        ])));
    }
}
