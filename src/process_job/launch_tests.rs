use super::*;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child as StandardChild, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

const MODE: &str = "SGT_PROCESS_LAUNCH_FIXTURE_MODE";
const READY: &str = "SGT_PROCESS_LAUNCH_FIXTURE_READY";

fn fixture() -> HelperCommand {
    let mut command = HelperCommand::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        &format!(
            "{}::native_child",
            module_path!().split_once("::").unwrap().1
        ),
        "--nocapture",
        "--",
    ]);
    command
}

#[test]
fn native_child() {
    match std::env::var(MODE).as_deref() {
        Ok("report") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            assert!(
                unsafe { windows::Win32::System::Console::GetConsoleWindow() }
                    .0
                    .is_null()
            );
            println!(
                "launch-report:{}",
                serde_json::json!({
                    "args":std::env::args().skip_while(|value|value!="--").skip(1).collect::<Vec<_>>(),
                    "directory":std::env::current_dir().unwrap(), "input":input,
                    "value":std::env::var("SGT_PROCESS_LAUNCH_VALUE").ok(),
                    "removed":std::env::var_os("SGT_PROCESS_LAUNCH_REMOVED").is_none(),
                    "pathAbsent":std::env::var_os("PATH").is_none(),
                })
            );
            eprintln!("launch-error-stream");
            std::process::exit(0);
        }
        Ok("parent") => {
            let mut command = fixture();
            command.env(MODE, "leaf").stdout_null().stderr_null();
            let child = OwnedProcess::spawn_helper(&command, ()).unwrap();
            assert!(
                child
                    .control()
                    .contains_process(HANDLE(child.as_raw_handle()))
                    .unwrap()
            );
            let ready = PathBuf::from(std::env::var_os(READY).unwrap());
            let pending = ready.with_extension("pending");
            std::fs::write(&pending, child.id().to_string()).unwrap();
            std::fs::rename(pending, ready).unwrap();
            // Keep the child suspended: parent death before resume must still
            // close a job that already owns the exact child process.
            loop {
                std::thread::park();
            }
        }
        Ok("leaf") => loop {
            std::thread::park();
        },
        _ => {}
    }
}

fn scratch() -> PathBuf {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let nonce = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let root = std::env::temp_dir().join(format!(
        "sgt-process-launch-{}-{nonce} space",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    root
}

fn wait_natural(child: &mut OwnedProcess) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "launch fixture did not exit");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn atomic_launch_preserves_cleared_environment_unicode_cwd_and_bidirectional_pipes() {
    let root = scratch();
    let mut command = fixture();
    let arguments = ["two words", "quote\"inside", "C:\\a b\\", "資料 λ"];
    command
        .args(arguments)
        .env("SGT_PROCESS_LAUNCH_REMOVED", "old")
        .env_clear()
        .env(MODE, "report")
        .env("SystemRoot", std::env::var_os("SystemRoot").unwrap())
        .env("SGT_PROCESS_LAUNCH_VALUE", "one=two λ")
        .current_dir(&root)
        .pipe_stdin();
    let mut child = OwnedProcess::spawn_helper(&command, ()).unwrap();
    assert!(
        child
            .control()
            .contains_process(HANDLE(child.as_raw_handle()))
            .unwrap()
    );
    assert_eq!(child.active_processes().unwrap(), 1);
    let mut input = child.take_stdin().unwrap();
    let mut output = child.take_stdout().unwrap();
    let mut errors = child.take_stderr().unwrap();
    child.resume().unwrap();
    input.write_all("input λ\n".as_bytes()).unwrap();
    drop(input);
    assert!(wait_natural(&mut child).success());
    child.stop_within(Duration::from_secs(3)).unwrap();
    let mut body = String::new();
    output.read_to_string(&mut body).unwrap();
    let report: serde_json::Value = serde_json::from_str(
        body.lines()
            .find_map(|line| line.strip_prefix("launch-report:"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report["args"], serde_json::json!(arguments));
    assert_eq!(
        Path::new(report["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(report["input"], "input λ\n");
    assert_eq!(report["value"], "one=two λ");
    assert_eq!(report["removed"], true);
    assert_eq!(report["pathAbsent"], true);
    let mut error = String::new();
    errors.read_to_string(&mut error).unwrap();
    assert!(error.contains("launch-error-stream"));
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn null_output_and_error_are_explicit_native_sinks() {
    let mut command = fixture();
    command.env(MODE, "report").stdout_null().stderr_null();
    let mut child = OwnedProcess::spawn_helper(&command, ()).unwrap();
    assert!(child.take_stdin().is_none());
    assert!(child.take_stdout().is_none());
    assert!(child.take_stderr().is_none());
    child.resume().unwrap();
    assert!(wait_natural(&mut child).success());
    child.stop_within(Duration::from_secs(3)).unwrap();
}

struct Released(Arc<AtomicBool>);
impl Drop for Released {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn replacement_permit_and_resources_survive_until_suspended_process_exit_is_proved() {
    static SLOT: LaunchSlot = LaunchSlot::new();
    let released = Arc::new(AtomicBool::new(false));
    let mut command = fixture();
    command.env(MODE, "leaf");
    let child = OwnedProcess::spawn_helper(
        &command,
        (SLOT.acquire().unwrap(), Released(released.clone())),
    )
    .unwrap();
    assert!(SLOT.acquire().is_err());
    assert!(!released.load(Ordering::Acquire));
    let handle = unsafe { std::os::windows::io::BorrowedHandle::borrow_raw(child.as_raw_handle()) }
        .try_clone_to_owned()
        .unwrap();
    drop(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !released.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "retired launch resources were not released"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 0) },
        WAIT_OBJECT_0
    );
    assert!(SLOT.acquire().is_ok());
}

struct ChildGuard(StandardChild);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn creator_death_before_resume_cannot_leave_an_unassigned_child() {
    let root = scratch();
    let ready = root.join("ready.pid");
    let mut command = crate::system_command::background_command(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!(
                "{}::native_child",
                module_path!().split_once("::").unwrap().1
            ),
        ])
        .env(MODE, "parent")
        .env(READY, &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut parent = ChildGuard(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.is_file() {
        assert!(
            parent.0.try_wait().unwrap().is_none(),
            "creator fixture exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "creator fixture did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let body = std::fs::read(&ready).unwrap();
    let pid = String::from_utf8(body.clone())
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let process = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    }
    .unwrap();
    let process = unsafe { OwnedHandle::from_raw_handle(process.0) };
    parent.0.kill().unwrap();
    parent.0.wait().unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 10_000) },
        WAIT_OBJECT_0
    );
    assert_eq!(std::fs::read(&ready).unwrap(), body);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_file(ready).unwrap();
    std::fs::remove_dir(root).unwrap();
}
