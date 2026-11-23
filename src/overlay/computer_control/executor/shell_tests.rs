use super::contract::invokes_inline_interpreter;
use super::{
    CapturedOutput, ProcessSpec, command_arg, command_working_dir, http_url_arg, output_value,
    run_command_with_timeout,
};
use serde_json::json;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn effect_assessment_uses_the_same_process_parser_and_canonical_working_directory() {
    let cwd = std::env::temp_dir();
    let args = json!({"program":std::env::current_exe().unwrap(),"args":["relative-resource.csv"],"cwd":cwd});
    let context = super::scope_context(&args).unwrap();
    assert_eq!(context["mode"], "exact_process");
    assert_eq!(
        context["cwd"],
        std::fs::canonicalize(cwd).unwrap().to_str().unwrap()
    );
    assert_eq!(context["args"], args["args"]);
    assert!(super::scope_context(&json!({"program":"future","command":"opaque"})).is_err());
}

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
    let program = crate::system_command::system_executable("where.exe").unwrap();
    let directory = program.parent().unwrap().to_str().unwrap();
    let argv = json!([format!("{directory}:cmd.exe")]);
    let result = run_command_with_timeout(
        &json!({"program": program, "args": argv, "cwd": cwd}),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "process receipt: {result}");
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
    assert_eq!(result["cwd"], json!(cwd));
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
fn explicit_extensionless_program_resolves_a_supported_windows_shim() {
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
        &json!({"program": "./probe", "args": ["literal value"], "cwd": root}),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["stdout"], "literal value\r\n");
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
fn free_form_command_output_is_utf8_regardless_of_the_oem_code_page() {
    // Built from code points so the command line itself stays ASCII.
    let result = run_command_with_timeout(
        &json!({"command": "$text = [string][char]0xD55C + [char]0xAE00 + [char]0x00E9 + [char]0x1EA1; \
            Write-Output $text; [Console]::Error.WriteLine($text); \
            if ($OutputEncoding.CodePage -ne 65001) { exit 3 }"}),
        Duration::from_secs(10),
    )
    .unwrap();
    let expected = "\u{D55C}\u{AE00}\u{00E9}\u{1EA1}";
    assert_eq!(result["ok"], true, "command receipt: {result}");
    assert_eq!(result["stdout"].as_str().unwrap().trim_end(), expected);
    assert_eq!(result["stderr"].as_str().unwrap().trim_end(), expected);
    assert_eq!(result["stdout_lossy"], false);
    assert_eq!(result["stderr_lossy"], false);
}

#[test]
fn free_form_command_keeps_its_own_exit_status_after_the_encoding_prelude() {
    let result = run_command_with_timeout(
        &json!({"command": "Write-Output first\nexit 7"}),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(result["exit_code"], 7);
    assert_eq!(result["stdout"].as_str().unwrap().trim_end(), "first");
    assert_eq!(result["stderr"], "");
}

#[test]
fn free_form_command_may_begin_with_using_statements() {
    let result = run_command_with_timeout(
        &json!({"command": "# imports\nusing namespace System.Text\n\
            $builder = [StringBuilder]::new(); [void]$builder.Append([char]0xD55C); \
            Write-Output $builder.ToString()"}),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "command receipt: {result}");
    assert_eq!(result["stdout"].as_str().unwrap().trim_end(), "\u{D55C}");
    assert_eq!(result["stderr"], "");
}

#[test]
fn free_form_command_may_begin_with_a_param_block() {
    let result = run_command_with_timeout(
        &json!({"command": "[CmdletBinding()]\nparam([string]$Suffix = 'a)b')\n\
            Write-Output ([string][char]0xD55C + $Suffix)"}),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "command receipt: {result}");
    assert_eq!(result["stdout"].as_str().unwrap().trim_end(), "\u{D55C}a)b");
    assert_eq!(result["stderr"], "");
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
    let result = output_value(&CapturedOutput::complete(output));
    assert_eq!(result["ok"], true);
    assert_eq!(result["ok_scope"], "process_exit_status");
    assert_eq!(result["process_completed"], true);
    assert_eq!(result["effect_verified"], false);
    assert_eq!(result["effect_may_have_occurred"], true);
}

#[test]
fn inline_interpreter_options_after_option_values_or_in_flag_groups_are_rejected() {
    for (program, args) in [
        (
            "powershell.exe",
            vec!["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "x"],
        ),
        ("pwsh", vec!["-WindowStyle", "Hidden", "-c", "x"]),
        (
            "powershell",
            vec!["-ep", "Bypass", "-EncodedCommand", "eA=="],
        ),
        (
            "powershell.exe",
            vec!["-ExecutionPolicy:Bypass", "-Command", "x"],
        ),
        ("python.exe", vec!["-W", "ignore", "-c", "x"]),
        ("python", vec!["-X", "utf8", "-c", "x"]),
        ("python3", vec!["-Bc", "x"]),
        ("bash", vec!["-o", "pipefail", "-c", "x"]),
        ("perl", vec!["-ne", "print"]),
        ("ruby", vec!["-pe", "x"]),
        ("node", vec!["-r", "module", "-e", "x"]),
    ] {
        assert!(
            invokes_inline_interpreter(
                program,
                &args.into_iter().map(str::to_string).collect::<Vec<_>>()
            ),
            "{program} inline code was accepted as an exact process"
        );
    }
    for (program, args) in [
        (
            "pwsh",
            vec!["-ExecutionPolicy", "Bypass", "-File", "probe.ps1", "-c"],
        ),
        ("python", vec!["-W", "ignore", "script.py", "-c"]),
        ("python", vec!["-m", "module", "-c"]),
        ("perl", vec!["-Mstrict", "script.pl"]),
        ("bash", vec!["-o", "pipefail", "probe.sh", "-c"]),
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
fn host_shells_reject_inline_code_in_every_form_they_parse() {
    // cmd scans the whole command line for /C, /K and /R, accepts a switch
    // glued to its command (`/cecho`) and chains switches (`/d/c`). Windows
    // PowerShell binds a bare positional to -Command.
    for (program, args) in [
        ("cmd.exe", vec!["/cecho", "fabricated"]),
        ("cmd", vec!["/d/c", "echo fabricated"]),
        ("cmd", vec!["/R", "echo fabricated"]),
        ("cmd", vec!["/E:ON/V:ON/cecho", "fabricated"]),
        ("cmd", vec!["positional", "/c", "echo fabricated"]),
        (
            "powershell.exe",
            vec!["-NoProfile", "Write-Output", "fabricated"],
        ),
        ("powershell", vec!["Write-Output fabricated"]),
    ] {
        assert!(
            invokes_inline_interpreter(
                program,
                &args.into_iter().map(str::to_string).collect::<Vec<_>>()
            ),
            "{program} inline code was accepted as an exact process"
        );
    }
    for (program, args) in [
        ("pwsh", vec!["-NoProfile", "probe.ps1"]),
        ("cmd.exe", vec!["/q", "/d"]),
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
