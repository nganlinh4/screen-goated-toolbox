use super::arguments::StartupArgs;
use std::fmt::Display;

const GT_NARRATION_TEST_FLAG: &str = "--gt-narration-test";

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

    #[test]
    fn computer_control_flags_are_not_headless_entrances() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../parity-fixtures/phone-control/external-control.json"
        ))
        .expect("external-control fixture is JSON");
        let flags = fixture["removedComputerControlCommandLineFlags"]
            .as_array()
            .expect("the fixture lists the removed Computer Control flags");
        assert!(!flags.is_empty());
        for flag in flags {
            let flag = flag.as_str().expect("each removed flag is a string");
            let args = StartupArgs::for_test(&["sgt.exe", flag, "input"]);
            assert_eq!(
                run_after_bootstrap(&args),
                None,
                "post-bootstrap flag={flag}"
            );
        }
    }
}
