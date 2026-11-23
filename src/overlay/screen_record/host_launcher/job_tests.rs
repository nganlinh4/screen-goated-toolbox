use super::*;
use std::io::{BufRead, BufReader};
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::JobObjects::{
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use session_lifecycle::{ActiveJobRegistration, JobSlot};

/// Tests that register worker jobs share the process-wide slot table.
static SLOT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial_slots() -> std::sync::MutexGuard<'static, ()> {
    SLOT_TEST_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner())
}

fn shell() -> ProcessCommand {
    let path = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])
    .unwrap();
    let mut command = crate::system_command::background_command(path);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 60",
    ]);
    command
}

#[test]
fn actual_worker_job_keeps_kill_on_close_and_browser_breakaway() {
    let mut worker = spawn_worker_process(&mut shell(), ()).unwrap();
    let flags = worker
        .control()
        .limits()
        .unwrap()
        .BasicLimitInformation
        .LimitFlags;
    assert!(flags.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE));
    assert!(flags.contains(JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK));
    worker.stop_within(Duration::from_secs(3)).unwrap();
}

struct Released(Arc<AtomicBool>);
impl Drop for Released {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn recorder_removal_stops_breakaway_helpers_before_releasing_resources() {
    const CHILD: &str = "SGT_RECORDER_HELPER_OWNER_TEST";
    let name = format!(
        "{}::recorder_removal_stops_breakaway_helpers_before_releasing_resources",
        module_path!().split_once("::").unwrap().1
    );
    if std::env::var(CHILD).as_deref() == Ok(name.as_str()) {
        crate::process_job::initialize_parent_job().unwrap();
        let path = crate::system_command::system_relative_executable(&[
            "WindowsPowerShell",
            "v1.0",
            "powershell.exe",
        ])
        .unwrap();
        let mut children = Vec::new();
        for piped in [false, true] {
            let mut command = crate::process_job::HelperCommand::new(&path);
            command.args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 60",
            ]);
            if piped {
                command.pipe_stdin();
            }
            let mut helper = OwnedProcess::spawn_helper(&command, ()).unwrap();
            let input = helper.take_stdin();
            assert_eq!(input.is_some(), piped);
            assert_eq!(
                helper.active_processes().unwrap(),
                1,
                "the suspended helper must already belong to its job"
            );
            helper.resume().unwrap();
            println!("helper-ready:{}", helper.id());
            children.push((helper, input));
        }
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    let _serial = serial_slots();
    let job_name = format!("Local\\SGTHelperTest-{}", launch_token().unwrap());
    let helpers = JobControl::named(&job_name).unwrap();
    assert!(
        JobControl::named(&job_name).is_err(),
        "an existing job identity must not be reused"
    );
    let flags = helpers.limits().unwrap().BasicLimitInformation.LimitFlags;
    assert!(flags.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE));
    assert!(!flags.contains(JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK));
    let released:[Arc<AtomicBool>;4]=std::array::from_fn(|_|Arc::new(AtomicBool::new(false)));
    let mut command = crate::system_command::background_command(std::env::current_exe().unwrap());
    command
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, &name)
        .env(crate::process_job::PARENT_JOB_ENV, &job_name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let retained=released.iter().map(|flag|Released(flag.clone())).collect::<Vec<_>>();
    let mut worker = spawn_worker_process(&mut command,retained).unwrap();
    worker.retain_job(helpers.clone());
    worker.retain(ActiveJobRegistration::register(worker.group(), JobSlot::Interactive).unwrap());
    let stdout = worker.take_stdout().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut processes = Vec::new();
        for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
            if let Some(pid) = line
                .strip_prefix("helper-ready:")
                .and_then(|pid| pid.parse::<u32>().ok())
            {
                processes.push(pid);
                if processes.len() == 2 {
                    let _ = send.send(processes);
                    return;
                }
            }
        }
    });
    assert_eq!(
        helpers.active_processes().unwrap(),
        0,
        "suspended workers must not launch helpers"
    );
    worker.resume().unwrap();
    let pids = receive.recv_timeout(Duration::from_secs(15)).unwrap();
    let mut processes = Vec::new();
    let member_ids = helpers.member_ids().unwrap();
    assert!(pids.iter().all(|pid| member_ids.contains(pid)));
    assert!(helpers.active_processes().unwrap() >= pids.len() as u32);
    for pid in member_ids {
        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .unwrap();
        let process = unsafe { OwnedHandle::from_raw_handle(process.0) };
        let mut image = vec![0_u16; 32768];
        let mut length = image.len() as u32;
        unsafe {
            windows::Win32::System::Threading::QueryFullProcessImageNameW(
                HANDLE(process.as_raw_handle()),
                Default::default(),
                windows::core::PWSTR(image.as_mut_ptr()),
                &mut length,
            )
        }
        .unwrap();
        let path = PathBuf::from(String::from_utf16_lossy(&image[..length as usize]));
        eprintln!("Contained fixture member {pid}: {:?}", path.file_name());
        assert!(
            helpers
                .contains_process(HANDLE(process.as_raw_handle()))
                .unwrap()
        );
        assert!(
            !worker
                .control()
                .contains_process(HANDLE(process.as_raw_handle()))
                .unwrap(),
            "WebView breakaway remains enabled"
        );
        processes.push(process);
    }
    assert!(released.iter().all(|flag|!flag.load(Ordering::SeqCst)));
    session_lifecycle::cancel_active_work();
    worker.stop_within(Duration::from_secs(5)).unwrap();
    for process in processes {
        assert_eq!(
            unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) },
            WAIT_OBJECT_0
        );
    }
    assert!(released.iter().all(|flag|!flag.load(Ordering::SeqCst)));
    drop(worker);
    assert!(released.iter().all(|flag|flag.load(Ordering::SeqCst)));
    assert!(!session_lifecycle::active_job_registered());
    assert_eq!(helpers.active_processes().unwrap(), 0);
}

#[test]
fn headless_job_runs_beside_the_interactive_window_and_stops_alone() {
    let _serial = serial_slots();
    let mut window = spawn_worker_process(&mut shell(), ()).unwrap();
    window.retain(ActiveJobRegistration::register(window.group(), JobSlot::Interactive).unwrap());
    let mut decoder = spawn_worker_process(&mut shell(), ()).unwrap();
    decoder.retain(
        ActiveJobRegistration::register(decoder.group(), JobSlot::Headless)
            .expect("an audio decode must not collide with the open recorder window"),
    );
    let mut second = spawn_worker_process(&mut shell(), ()).unwrap();
    assert!(
        ActiveJobRegistration::register(second.group(), JobSlot::Headless).is_err(),
        "headless jobs still run one at a time"
    );
    second.stop_within(Duration::from_secs(5)).unwrap();

    session_lifecycle::cancel_slot_work(JobSlot::Headless);
    let deadline = Instant::now() + Duration::from_secs(5);
    while decoder.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the headless worker was not stopped");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        window.try_wait().unwrap().is_none(),
        "stopping a headless job must leave the recorder window running"
    );
    drop(decoder);
    window.stop_within(Duration::from_secs(5)).unwrap();
    drop(window);
    assert!(!session_lifecycle::active_job_registered());
}
