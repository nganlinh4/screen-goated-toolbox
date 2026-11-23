use super::*;
use std::process::{Command, Stdio};
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};

const LEASE_ID: &str = "test-creation-process-owner";

fn isolated(name: &str, test: impl FnOnce()) {
    let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
    crate::overlay::creation_submission_test::run_isolated(&name, |_| test());
}

fn spawn(script: &str) -> (CreationProcess, EventSupervisor) {
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
        .env("SGT_TEST_SHELL_PATH", &shell)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let lease = crate::component_registry::acquire(LEASE_ID).unwrap();
    let mut child = CreationProcess::spawn(&mut command, lease).unwrap();
    let stdout = child.take_stdout().unwrap();
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 15_000;
    let events = EventSupervisor::with_deadline(stdout, deadline).unwrap();
    child.resume().unwrap();
    (child, events)
}

const DESCENDANT_SCRIPT: &str = r#"
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = $env:SGT_TEST_SHELL_PATH
$info.Arguments = '-NoLogo -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 60"'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$leaf = [System.Diagnostics.Process]::Start($info)
[Console]::Out.WriteLine(('{"ok":true,"descendant":' + $leaf.Id + '}'))
Start-Sleep -Seconds 60
"#;

#[test]
fn terminal_drain_releases_stdout_backpressure_and_allows_normal_exit() {
    isolated(
        "terminal_drain_releases_stdout_backpressure_and_allows_normal_exit",
        || {
            let (mut child, mut events) = spawn(
                r#"
[Console]::Out.WriteLine('{"ok":true,"result":{"value":7}}')
$event = '{"event":"log","data":"' + ('x' * 512) + '"}'
for ($item = 0; $item -lt 2048; $item++) { [Console]::Out.WriteLine($event) }
"#,
            );
            let terminal = events.next(child.id(), || false).unwrap().unwrap();
            assert_eq!(terminal["result"]["value"], 7);
            events.finish(&mut child).unwrap();
            assert!(
                child.try_wait().unwrap().unwrap().success(),
                "draining must permit a normal exit"
            );
            assert_eq!(child.active_processes().unwrap(), 0);
            assert_eq!(terminal["result"]["value"], 7);
        },
    );
}

#[test]
fn terminal_timeout_stops_and_reaps_the_owned_descendant_tree() {
    isolated(
        "terminal_timeout_stops_and_reaps_the_owned_descendant_tree",
        || {
            let (mut child, mut events) = spawn(DESCENDANT_SCRIPT);
            let terminal = events.next(child.id(), || false).unwrap().unwrap();
            let descendant = terminal["descendant"].as_u64().unwrap() as u32;
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, descendant) }.unwrap();
            assert!(child.active_processes().unwrap() >= 2);
            let started = Instant::now();
            events
                .finish_with_limits(
                    &mut child,
                    Duration::from_millis(20),
                    Duration::from_secs(3),
                )
                .unwrap();
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(child.active_processes().unwrap(), 0);
            assert!(child.try_wait().unwrap().is_some());
            unsafe {
                assert_eq!(WaitForSingleObject(handle, 0), WAIT_OBJECT_0);
                CloseHandle(handle).unwrap();
            }
        },
    );
}

#[test]
fn cancellation_addresses_the_owned_job_and_stops_descendants() {
    isolated(
        "cancellation_addresses_the_owned_job_and_stops_descendants",
        || {
            let (mut child, mut events) = spawn(DESCENDANT_SCRIPT);
            events.next(child.id(), || false).unwrap().unwrap();
            let pid = child.id();
            crate::overlay::creation_recovery::terminate_process_tree(pid);
            events
                .finish_with_limits(
                    &mut child,
                    Duration::from_millis(20),
                    Duration::from_secs(3),
                )
                .unwrap();
            assert_eq!(child.active_processes().unwrap(), 0);
            drop(child);
            assert!(crate::overlay::creation_process::terminate_owned(pid).is_none());
        },
    );
}

#[test]
fn dropping_an_active_owner_retains_its_lease_until_processes_are_stopped() {
    isolated(
        "dropping_an_active_owner_retains_its_lease_until_processes_are_stopped",
        || {
            let (child, mut events) = spawn(DESCENDANT_SCRIPT);
            events.next(child.id(), || false).unwrap().unwrap();
            let control = child.control();
            let pid = child.id();
            assert_eq!(
                crate::component_registry::request_remove(LEASE_ID).unwrap(),
                crate::component_registry::RemovalOutcome::Pending
            );
            assert!(crate::component_registry::acquire(LEASE_ID).is_err());
            drop(child);
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if crate::component_registry::request_remove(LEASE_ID).unwrap()
                    == crate::component_registry::RemovalOutcome::Missing
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "component lease was not retired after process cleanup"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(control.active_processes().unwrap(), 0);
            assert!(crate::overlay::creation_process::terminate_owned(pid).is_none());
            assert!(crate::component_registry::acquire(LEASE_ID).is_ok());
        },
    );
}
