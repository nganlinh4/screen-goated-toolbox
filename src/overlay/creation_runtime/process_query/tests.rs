use super::*;

fn isolated(name: &str, test: impl FnOnce(&std::path::Path)) {
    let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
    crate::overlay::creation_submission_test::run_isolated(&name, test);
}

fn command(script: &str) -> Command {
    let shell = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])
    .unwrap();
    let mut command = Command::new(&shell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("SGT_TEST_SHELL_PATH", shell);
    command
}

#[test]
fn query_writes_its_input_and_keeps_only_the_bounded_output_tail() {
    isolated(
        "query_writes_its_input_and_keeps_only_the_bounded_output_tail",
        |_| {
            let mut command =
                command("$line = [Console]::In.ReadLine(); [Console]::Out.Write($line)");
            let output = run(
                &mut command,
                Some(b"1234567890\n"),
                Duration::from_secs(5),
                4,
            )
            .unwrap();
            assert!(output.status.success());
            assert!(output.truncated);
            assert_eq!(output.bytes, b"7890");
            assert!(super::super::RUNTIME_PROCESSES.lock().unwrap().is_empty());
        },
    );
}

#[test]
fn a_query_that_never_reads_input_still_obeys_its_deadline() {
    isolated(
        "a_query_that_never_reads_input_still_obeys_its_deadline",
        |_| {
            let mut command = command("Start-Sleep -Seconds 60");
            let started = Instant::now();
            assert!(
                run(
                    &mut command,
                    Some(&vec![b'x'; 1024 * 1024]),
                    Duration::from_millis(100),
                    64
                )
                .is_none()
            );
            assert!(started.elapsed() < Duration::from_secs(4));
            assert!(super::super::RUNTIME_PROCESSES.lock().unwrap().is_empty());
        },
    );
}

#[test]
fn query_exit_does_not_leave_an_owned_descendant_running() {
    isolated(
        "query_exit_does_not_leave_an_owned_descendant_running",
        |root| {
            use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
            use windows::Win32::System::Threading::{
                OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
            };
            let pid_file = root.join("descendant.pid");
            let ack_file = root.join("observed");
            let mut command = command(
                r#"
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = $env:SGT_TEST_SHELL_PATH
$info.Arguments = '-NoLogo -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 60"'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$leaf = [System.Diagnostics.Process]::Start($info)
[System.IO.File]::WriteAllText($env:SGT_TEST_DESCENDANT_PID, [string]$leaf.Id)
$until = [DateTime]::UtcNow.AddSeconds(10)
while (-not [System.IO.File]::Exists($env:SGT_TEST_OBSERVED) -and [DateTime]::UtcNow -lt $until) { [Threading.Thread]::Sleep(10) }
[Console]::Out.Write('done')
"#,
            );
            command
                .env("SGT_TEST_DESCENDANT_PID", &pid_file)
                .env("SGT_TEST_OBSERVED", &ack_file);
            let query =
                std::thread::spawn(move || run(&mut command, None, Duration::from_secs(5), 64));
            let deadline = Instant::now() + Duration::from_secs(5);
            let pid = loop {
                if let Ok(value) = std::fs::read_to_string(&pid_file)
                    && let Ok(pid) = value.parse::<u32>()
                {
                    break pid;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            };
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.unwrap();
            std::fs::write(ack_file, "observed").unwrap();
            let _output = query.join().unwrap();
            unsafe {
                assert_eq!(WaitForSingleObject(handle, 5_000), WAIT_OBJECT_0);
                CloseHandle(handle).unwrap();
            }
            assert!(super::super::RUNTIME_PROCESSES.lock().unwrap().is_empty());
        },
    );
}
