use super::*;
use crate::overlay::creation_process_supervisor::EventSupervisor;
use std::io::Write as _;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn command(script: &str) -> Command {
    let shell = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])
    .unwrap();
    let mut command = Command::new(shell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

fn stop(child: &mut CreationProcess) {
    child.terminate().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !child.0.as_mut().unwrap().stopped() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn frozen_cancellation_cannot_follow_a_reused_numeric_process_id() {
    let name = format!(
        "{}::frozen_cancellation_cannot_follow_a_reused_numeric_process_id",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |_| {
        let lease =
            || crate::component_registry::acquire("test-process-cancellation-owner").unwrap();
        let mut old =
            CreationProcess::spawn(&mut command("Start-Sleep -Seconds 60"), lease()).unwrap();
        let retired_pid = old.id();
        let stale_registration = Registration {
            pid: retired_pid,
            control: old.control(),
        };
        let frozen = Termination::capture(retired_pid);
        stop(&mut old);
        drop(old);
        let mut current = CreationProcess::spawn(
            &mut command(
                r#"
[Console]::Out.WriteLine('{"event":"ready"}')
$null = [Console]::In.ReadLine()
[Console]::Out.WriteLine('{"event":"alive"}')
Start-Sleep -Seconds 60
"#,
            ),
            lease(),
        )
        .unwrap();
        let stdout = current.take_stdout().unwrap();
        let mut input = current.take_stdin().unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 15_000;
        let mut events = EventSupervisor::with_deadline(stdout, deadline).unwrap();
        current.resume().unwrap();
        assert_eq!(
            events.next(current.id(), || false).unwrap().unwrap()["event"],
            "ready"
        );
        // Model PID reuse without relying on the OS to recycle a number.
        JOBS.lock().unwrap().insert(retired_pid, current.control());
        drop(stale_registration);
        assert!(
            JOBS.lock()
                .unwrap()
                .get(&retired_pid)
                .unwrap()
                .same_job(&current.control())
        );
        frozen.terminate();
        writeln!(input).unwrap();
        input.flush().unwrap();
        assert_eq!(
            events.next(current.id(), || false).unwrap().unwrap()["event"],
            "alive"
        );
        JOBS.lock().unwrap().remove(&retired_pid);
        stop(&mut current);
    });
}
