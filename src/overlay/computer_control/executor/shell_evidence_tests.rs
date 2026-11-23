use super::*;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::ExitStatusExt;

fn captured(bytes: Vec<u8>, complete: bool) -> CapturedOutput {
    CapturedOutput {
        output: std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: bytes,
            stderr: Vec::new(),
        },
        stdout_complete: complete,
        stderr_complete: true,
    }
}

fn spec(cwd: PathBuf) -> ProcessSpec {
    ProcessSpec {
        program: "example.exe".into(),
        args: vec!["argument".into()],
        cwd,
    }
}

fn hex_bytes(value: &str) -> Vec<u8> {
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn shared_encoding_cases_preserve_bytes_without_claiming_text_exactness() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/phone-control/command-output-encoding.json"
    ))
    .unwrap();
    assert_eq!(fixture["raw_prefix_limit_bytes"], RAW_PREFIX_BYTES);
    for case in fixture["cases"].as_array().unwrap() {
        let bytes = hex_bytes(case["stdout_hex"].as_str().unwrap());
        let complete = case["source_complete"].as_bool().unwrap_or(true);
        let capture = captured(bytes.clone(), complete);
        for result in [
            output_value(&capture),
            process_output_value(&capture, &spec(PathBuf::from("C:\\work"))),
        ] {
            assert_eq!(
                result["stdout_lossy"], case["utf8_lossy"],
                "{}",
                case["name"]
            );
            assert_eq!(result["stdout_text_exact"], case["text_exact"]);
            assert_eq!(result["stdout_bytes_complete"], case["raw_complete"]);
            assert_eq!(result["stdout_encoding"], "unknown_or_mixed");
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(result["stdout_bytes_base64"].as_str().unwrap())
                    .unwrap(),
                bytes
            );
        }
        let result = process_output_value(&capture, &spec(PathBuf::from("C:\\work")));
        assert!(
            result["completion_proof"]["partial"]
                .as_array()
                .unwrap()
                .contains(&json!("/stdout"))
        );
        assert_eq!(
            result["completion_proof"]["exact"]
                .as_array()
                .unwrap()
                .contains(&json!("/stdout_bytes_base64")),
            complete
        );
    }
}

#[test]
fn oversized_prefix_is_partial_and_bounded() {
    let capture = captured(vec![b'x'; RAW_PREFIX_BYTES + 1], true);
    let result = process_output_value(&capture, &spec(PathBuf::from("C:\\work")));
    assert_eq!(result["stdout_bytes_complete"], false);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["stdout_bytes_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes.len(), RAW_PREFIX_BYTES);
    assert!(
        result["completion_proof"]["partial"]
            .as_array()
            .unwrap()
            .contains(&json!("/stdout_bytes_base64"))
    );
}

#[test]
fn non_unicode_working_directory_is_not_exact_text() {
    let path = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    let result = process_output_value(&captured(Vec::new(), true), &spec(path));
    assert_eq!(result["cwd_lossy"], true);
    assert!(
        result["completion_proof"]["partial"]
            .as_array()
            .unwrap()
            .contains(&json!("/cwd"))
    );
    assert!(
        !result["completion_proof"]["exact"]
            .as_array()
            .unwrap()
            .contains(&json!("/cwd"))
    );
}

#[test]
fn typical_max_text_and_raw_prefixes_fit_the_response_budget() {
    let mut capture = captured(vec![1; 4_000], true);
    capture.output.stderr = vec![1; 4_000];
    let result = process_output_value(&capture, &spec(PathBuf::from("C:\\work")));
    let prepared =
        crate::overlay::computer_control::response_budget::prepare("id", "run_command", result)
            .unwrap();
    assert!(!prepared.omitted);
}
