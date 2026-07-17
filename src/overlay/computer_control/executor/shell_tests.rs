use super::contract::invokes_inline_interpreter;
use super::{
    CommandCapture, ProcessSpec, command_arg, command_working_dir, http_url_arg, output_value,
    run_command_with_timeout,
};
use serde_json::json;
use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn command_content_is_never_classified_by_substring() {
    for command in [
        "Write-Output 'alpha beta'",
        "Get-Thing | Set-Thing -Mode 'Mixed-Case'",
        "Invoke-Tool 'C:\\path with spaces\\item'",
    ] {
        assert_eq!(command_arg(&json!({"command": command})).unwrap(), command);
    }
}

#[test]
fn command_field_is_still_structurally_required() {
    assert!(command_arg(&json!({})).is_err());
    assert!(command_arg(&json!({"command": 1})).is_err());
}

#[test]
fn process_mode_rejects_ambiguous_or_unbounded_shapes() {
    assert!(ProcessSpec::parse(&json!({"program": "cmd.exe", "command": "echo no"})).is_err());
    assert!(ProcessSpec::parse(&json!({"command": "echo no", "cwd": "C:\\"})).is_err());
    assert!(ProcessSpec::parse(&json!({"program": "cmd.exe", "cwd": "."})).is_err());
    assert!(
        ProcessSpec::parse(&json!({
            "program": "cmd.exe",
            "args": (0..17).map(|index| index.to_string()).collect::<Vec<_>>(),
        }))
        .is_err()
    );
}

#[test]
fn exact_process_rejects_inline_shell_code_without_launching_it() {
    for request in [
        json!({"program": "CMD.ExE", "args": ["/D", "/C", "echo changed"]}),
        json!({"program": "cmd", "args": ["/K", "echo changed"]}),
        json!({"program": "PowerShell.EXE", "args": ["-Command", "Set-Content x y"]}),
        json!({"program": "pwsh", "args": ["-ENC", "encoded"]}),
        json!({"program": "sh", "args": ["-c", "touch x"]}),
        json!({"program": "BASH.EXE", "args": ["-lc", "touch x"]}),
        json!({"program": "node.exe", "args": ["-e", "console.log('fabricated')"]}),
        json!({"program": "node", "args": ["--eval=console.log('fabricated')"]}),
        json!({"program": "python.exe", "args": ["-c", "print('fabricated')"]}),
        json!({"program": "python3.12.exe", "args": ["-c", "print('fabricated')"]}),
        json!({"program": "ruby", "args": ["-e", "puts 'fabricated'"]}),
        json!({"program": "php", "args": ["-r", "echo 'fabricated';"]}),
        json!({"program": "Rscript", "args": ["--expr", "cat('fabricated')"]}),
    ] {
        let result = run_command_with_timeout(&request, Duration::from_millis(1)).unwrap();
        assert_eq!(result["code"], "INLINE_INTERPRETER_NOT_EXACT_PROCESS");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(result["retryable"], true);
    }
}

#[test]
fn exact_process_keeps_direct_argv_and_non_inline_interpreters_available() {
    for (program, args) in [
        ("npm.cmd", vec!["test"]),
        ("node.exe", vec!["script.mjs", "-c"]),
        ("python.exe", vec!["script.py", "-c"]),
        ("ruby.exe", vec!["script.rb", "-e"]),
        ("future-runtime.exe", vec!["-e", "literal"]),
        ("git", vec!["status", "--short"]),
        ("cmd.exe", vec!["/d"]),
        ("powershell.exe", vec!["-NoProfile", "-File", "probe.ps1"]),
        ("powershell.exe", vec!["-File", "probe.ps1", "-Command"]),
        ("bash", vec!["probe.sh"]),
        ("bash", vec!["--", "-c"]),
    ] {
        assert!(
            !invokes_inline_interpreter(
                program,
                &args.into_iter().map(str::to_string).collect::<Vec<_>>()
            ),
            "{program} direct argv was rejected"
        );
    }
}

#[test]
fn implicit_command_working_directory_is_managed_scratch() {
    let expected = command_working_dir().unwrap();
    let spec = ProcessSpec::parse(&json!({"program": "cmd.exe"}))
        .unwrap()
        .unwrap();

    assert_eq!(spec.cwd, expected);
    assert!(spec.cwd.is_absolute());
    assert_ne!(
        spec.cwd,
        std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap()
    );
}

#[test]
fn exact_process_captures_invocation_exit_and_output() {
    let cwd = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let argv = json!(["cmd.exe"]);
    let result = run_command_with_timeout(
        &json!({"program": "where.exe", "args": argv, "cwd": cwd}),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["evidence_kind"], "exact_process_invocation");
    assert!(
        result["program"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with("where.exe")
    );
    assert_eq!(result["args"], argv);
    assert_eq!(result["arg_count"], 1);
    assert_eq!(result["exit_code"], 0);
    assert!(
        result["stdout"]
            .as_str()
            .is_some_and(|stdout| stdout.to_ascii_lowercase().contains("cmd.exe"))
    );
    assert_eq!(result["stderr"], "");
    assert_eq!(result["effect_verified"], false);
}

#[test]
fn extensionless_program_resolves_a_supported_windows_shim() {
    let root = std::env::temp_dir().join(format!(
        "sgt-cc-shim-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let shim = root.join("probe.cmd");
    std::fs::write(&shim, "@echo off\r\necho %~1\r\n").unwrap();
    let result = run_command_with_timeout(
        &json!({"program": "probe", "args": ["literal value"], "cwd": root}),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["stdout"], "literal value");
    assert!(
        result["program"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with("probe.cmd")
    );
    std::fs::remove_file(shim).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn exact_process_timeout_terminates_and_reaps_the_tree() {
    let started = Instant::now();
    let result = run_command_with_timeout(
        &json!({
            "program": "ping.exe",
            "args": ["-n", "30", "127.0.0.1"],
        }),
        Duration::from_millis(150),
    )
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["timed_out"], true);
    assert_eq!(result["terminated"], true);
    assert_eq!(result["reaped"], true);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn free_form_command_remains_non_exact_evidence() {
    let result = run_command_with_timeout(
        &json!({"command": "Write-Output alpha"}),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["ok_scope"], "process_exit_status");
    assert!(result.get("evidence_kind").is_none());
    assert!(result.get("completion_proof").is_none());
}

#[test]
fn open_url_accepts_only_absolute_http_targets() {
    for value in [
        json!({}),
        json!({"url": ""}),
        json!({"url": "relative"}),
        json!({"url": "C:\\local\\item.txt"}),
        json!({"url": "file:///C:/local/item.txt"}),
        json!({"url": "https://"}),
    ] {
        assert!(http_url_arg(&value).is_err());
    }
    assert_eq!(
        http_url_arg(&json!({"url": "https://example.invalid/path"})).unwrap(),
        "https://example.invalid/path"
    );
}

#[test]
fn shell_timeout_terminates_and_reaps_the_process_tree() {
    let started = Instant::now();
    let result = run_command_with_timeout(
        &json!({"command": "Start-Sleep -Seconds 20"}),
        Duration::from_millis(100),
    )
    .unwrap();
    assert_eq!(result["timed_out"], true);
    assert_eq!(result["terminated"], true);
    assert_eq!(result["reaped"], true);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn zero_exit_proves_process_completion_not_semantic_effect() {
    let output = Command::new("cmd")
        .args(["/c", "exit", "0"])
        .output()
        .unwrap();
    let result = output_value(&output);
    assert_eq!(result["ok"], true);
    assert_eq!(result["ok_scope"], "process_exit_status");
    assert_eq!(result["process_completed"], true);
    assert_eq!(result["effect_verified"], false);
    assert_eq!(result["effect_may_have_occurred"], true);
}

#[test]
fn file_capture_does_not_wait_for_writer_eof() {
    let (capture, mut stdout, mut stderr) = CommandCapture::create().unwrap();
    stdout.write_all(b"alpha").unwrap();
    stderr.write_all(b"beta").unwrap();
    stdout.flush().unwrap();
    stderr.flush().unwrap();
    let status = Command::new("cmd")
        .args(["/c", "exit", "0"])
        .status()
        .unwrap();
    let started = Instant::now();
    let output = capture.output(status).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(output.stdout, b"alpha");
    assert_eq!(output.stderr, b"beta");
}
