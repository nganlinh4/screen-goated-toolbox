use super::{
    CapturedOutput, MAX_CAPTURE_BYTES, ProcessSpec, job, process_output_value,
    run_command_cancellable,
};
use serde_json::json;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sgt-command-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn wait_for_file(path: &Path) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        if path.is_file() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn wait_for_pid(path: &Path) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        // Existence precedes Set-Content closing its handle. Wait for a readable,
        // complete PID before using it as evidence that the descendant is ready.
        let contents = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
            .and_then(|mut file| {
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                Ok(contents)
            });
        if let Ok(contents) = contents
            && let Ok(pid) = contents.trim().parse::<u32>()
            && pid != 0
        {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn already_cancelled_command_never_starts() {
    let result = run_command_cancellable(
        &json!({"command": "Write-Output example"}),
        Duration::from_secs(5),
        &AtomicBool::new(true),
    )
    .unwrap();
    assert_eq!(result["cancelled"], true);
    assert_eq!(result["not_started"], true);
    assert_eq!(result["effect_may_have_occurred"], false);
    assert_eq!(result["terminated"], false);
}

#[test]
fn suspended_child_cannot_run_before_containment_is_ready() {
    let scratch = Scratch::new();
    let marker = scratch.0.join("marker");
    let command = format!("Set-Content -LiteralPath '{}' -Value x", marker.display());
    let mut process = Command::new("powershell.exe");
    process.args(["-NoProfile", "-NonInteractive", "-Command", &command]);
    let mut child = job::ContainedChild::spawn(&mut process).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(!marker.exists());
    child.terminate().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!marker.exists());
}

#[test]
fn cancel_terminates_owned_descendant_and_returns_promptly() {
    use std::os::windows::io::AsRawHandle;
    let scratch = Scratch::new();
    let pid_path = scratch.0.join("child-pid");
    let command = format!(
        "$p = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\\cmd.exe') \
         -ArgumentList '/d','/c','ping -n 30 127.0.0.1 >nul' -WindowStyle Hidden -PassThru; \
         Set-Content -LiteralPath '{}' -Value $p.Id; Start-Sleep -Seconds 30",
        pid_path.display()
    );
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            run_command_cancellable(
                &json!({"command": command}),
                Duration::from_secs(30),
                &cancel,
            )
            .unwrap()
        });
        let pid = wait_for_pid(&pid_path);
        let ready = pid.is_some();
        let process = pid
            .and_then(|pid| unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok())
            .map(|handle| unsafe { OwnedHandle::from_raw_handle(handle.0) });
        let started = Instant::now();
        super::super::request_job_cancel(&cancel);
        let result = worker.join().unwrap();
        assert!(ready, "child did not start");
        let process = process.expect("open owned descendant before cancellation");
        assert_eq!(result["cancelled"], true);
        assert_eq!(result["effect_may_have_occurred"], true);
        assert_eq!(result["terminated"], true);
        assert_eq!(result["termination_requested"], true);
        assert_eq!(result["owned_processes_stopped"], true);
        assert_eq!(result["reaped"], true);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 5000) },
            WAIT_OBJECT_0
        );
    });
}

#[test]
fn exact_process_cancellation_preserves_invocation_receipt() {
    let scratch = Scratch::new();
    let marker = scratch.0.join("ready");
    let script = scratch.0.join("probe.cmd");
    std::fs::write(
        &script,
        format!(
            "@echo off\r\necho ready > \"{}\"\r\nping -n 30 127.0.0.1 >nul\r\n",
            marker.display()
        ),
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            run_command_cancellable(
                &json!({"program": script, "cwd": scratch.0}),
                Duration::from_secs(30),
                &cancel,
            )
            .unwrap()
        });
        let ready = wait_for_file(&marker);
        super::super::request_job_cancel(&cancel);
        let result = worker.join().unwrap();
        assert!(ready, "exact process did not start");
        assert_eq!(result["cancelled"], true);
        assert_eq!(result["effect_may_have_occurred"], true);
        assert_eq!(result["evidence_kind"], "exact_process_invocation");
        assert_eq!(result["reaped"], true);
        assert!(
            result["completion_proof"]["exact"]
                .as_array()
                .unwrap()
                .contains(&json!("/cancelled"))
        );
    });
}

#[test]
fn normal_completion_preserves_deliberately_launched_child() {
    use std::os::windows::io::AsRawHandle;
    let scratch = Scratch::new();
    let pid_path = scratch.0.join("child-pid");
    let command = format!(
        "$p = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') \
         -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30' \
         -WindowStyle Hidden -PassThru; Set-Content -LiteralPath '{}' -Value $p.Id",
        pid_path.display().to_string().replace('\'', "''")
    );
    let result = run_command_cancellable(
        &json!({"command": command}),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    let pid: u32 = std::fs::read_to_string(pid_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid) }.unwrap();
    let process = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let alive = unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) };
    let _ = unsafe { TerminateProcess(HANDLE(process.as_raw_handle()), 1) };
    let _ = unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 5000) };
    assert_eq!(alive, WAIT_TIMEOUT);
}

#[test]
fn capture_reads_are_bounded_and_report_truncation() {
    let result = run_command_cancellable(
        &json!({"command": format!("[Console]::Out.Write('x' * {})", MAX_CAPTURE_BYTES * 4)}),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["stdout_truncated"], true);
    assert_eq!(result["stdout"].as_str().unwrap().len(), 4000);
}

#[test]
fn output_preserves_whitespace_and_marks_text_encoding_uncertain() {
    let status = Command::new("cmd.exe")
        .args(["/c", "exit", "0"])
        .status()
        .unwrap();
    let spec = ProcessSpec::parse(&json!({"program": "where.exe"}))
        .unwrap()
        .unwrap();
    let captured = CapturedOutput::complete(std::process::Output {
        status,
        stdout: b"  line\r\n".to_vec(),
        stderr: vec![0xff],
    });
    let result = process_output_value(&captured, &spec);
    assert_eq!(result["stdout"], "  line\r\n");
    assert_eq!(result["stderr_lossy"], true);
    assert!(
        result["completion_proof"]["partial"]
            .as_array()
            .unwrap()
            .contains(&json!("/stdout"))
    );
    assert_eq!(result["stdout_text_exact"], false);
    assert_eq!(result["stdout_bytes_complete"], true);
    assert!(
        result["completion_proof"]["exact"]
            .as_array()
            .unwrap()
            .contains(&json!("/stdout_bytes_base64"))
    );
    assert!(
        result["completion_proof"]["partial"]
            .as_array()
            .unwrap()
            .contains(&json!("/stderr"))
    );
}

#[test]
fn command_processes_are_created_inside_their_job_before_they_can_run() {
    use std::os::windows::io::AsRawHandle;
    let scratch = Scratch::new();
    let marker = scratch.0.join("marker");
    let command = format!("Set-Content -LiteralPath '{}' -Value x", marker.display());
    let shell = super::spawn_powershell(&command, super::Reservation::acquire().unwrap()).unwrap();
    let program = crate::system_command::system_executable("where.exe").unwrap();
    let spec = ProcessSpec::parse(&json!({"program": program, "args": ["cmd.exe"]}))
        .unwrap()
        .unwrap();
    let exact = super::spawn_process(&spec, super::Reservation::acquire().unwrap()).unwrap();
    for captured in [&shell, &exact] {
        assert!(captured.child.assigned_at_creation());
        assert!(
            captured
                .child
                .control()
                .contains_process(HANDLE(captured.child.as_raw_handle()))
                .unwrap()
        );
    }
    std::thread::sleep(Duration::from_millis(150));
    assert!(!marker.exists());
    for captured in [shell, exact] {
        captured.child.terminate().unwrap();
    }
}

#[test]
fn batch_scripts_are_created_inside_their_job_before_they_can_run() {
    use std::os::windows::io::AsRawHandle;
    let scratch = Scratch::new();
    let script = scratch.0.join("run.cmd");
    let ran = scratch.0.join("ran");
    std::fs::write(&script, "@echo off\r\necho x> \"%~dp0ran\"\r\n").unwrap();
    let spec = ProcessSpec::parse(&json!({"program": script, "cwd": scratch.0}))
        .unwrap()
        .unwrap();
    let captured = super::spawn_process(&spec, super::Reservation::acquire().unwrap()).unwrap();
    assert!(captured.child.assigned_at_creation());
    assert!(
        captured
            .child
            .control()
            .contains_process(HANDLE(captured.child.as_raw_handle()))
            .unwrap()
    );
    std::thread::sleep(Duration::from_millis(150));
    assert!(!ran.exists());
    captured.child.terminate().unwrap();
}

#[test]
fn batch_script_arguments_reach_the_script_exactly_as_std_command_passes_them() {
    let scratch = Scratch::new();
    let script = scratch.0.join("echo args.cmd");
    std::fs::write(&script, "@echo off\r\necho(%*\r\n").unwrap();
    let arguments = [
        "plain",
        "two words",
        "a&b|c>d<e",
        "100%",
        "%PATH%",
        "q\"uote",
        "trail\\",
        "",
        "^caret",
        r"C:\dir\file.txt",
    ];
    let expected = Command::new(&script)
        .args(arguments)
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    assert!(expected.status.success());
    let result = run_command_cancellable(
        &json!({"program": script, "args": arguments, "cwd": scratch.0}),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "batch receipt: {result}");
    assert_eq!(
        result["stdout"].as_str().unwrap(),
        String::from_utf8_lossy(&expected.stdout),
        "batch receipt: {result}"
    );
}
