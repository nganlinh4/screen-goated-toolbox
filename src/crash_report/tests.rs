use super::*;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "sgt-crash-report-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn reports(&self) -> PathBuf {
        self.0.join("local-app-data/SGT/crashes")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(self.0.parent(), Some(temp.as_path()));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "diagnostic subprocess timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn run_probe(root: &TestRoot, mode: &str) -> (std::process::ExitStatus, PathBuf) {
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash_report::tests::diagnostic_child",
                "--ignored",
                "--nocapture",
            ])
            .env("SGT_RUNTIME_STATE_ROOT", &root.0)
            .env(
                "SGT_TEST_CRASH_MODE",
                if mode == "terminate" { "wait" } else { mode },
            )
            .env_remove("RUST_BACKTRACE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    if mode == "terminate" {
        wait_until(|| root.0.join("probe-ready").exists());
        child.0.kill().unwrap();
    }
    let mut status = None;
    wait_until(|| {
        status = child.0.try_wait().unwrap();
        status.is_some()
    });
    let directories: Vec<_> = std::fs::read_dir(root.reports())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), 1);
    let directory = directories[0].clone();
    wait_until(|| directory.join("exit.txt").exists());
    // sync_all precedes completion; wait for a complete status line rather
    // than accepting the moment an empty file is created.
    wait_until(|| {
        std::fs::read_to_string(directory.join("exit.txt"))
            .is_ok_and(|text| text.contains("exit_intent_recorded="))
    });
    (status.unwrap(), directory)
}

#[test]
fn caught_panic_returns_and_preserves_diagnostics() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "caught-panic");
    assert!(status.success());
    assert!(
        std::fs::read_to_string(directory.join("panic-0.txt"))
            .unwrap()
            .contains("caught-origin")
    );
    assert!(
        std::fs::read_to_string(directory.join("panic-0-stack.txt"))
            .unwrap()
            .contains("Stack:")
    );
    assert_dump(&directory.join("panic.dmp"), None);
}

#[test]
fn original_callback_panic_survives_non_unwinding_abort() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "callback-panic");
    assert!(!status.success());
    let original = std::fs::read_to_string(directory.join("panic-0.txt")).unwrap();
    assert!(original.contains("callback-origin"));
    let stack = std::fs::read_to_string(directory.join("panic-0-stack.txt")).unwrap();
    assert!(stack.contains("Stack:"));
    assert!(
        stack.contains("callback_origin"),
        "stack must include the originating callback: {stack}"
    );
    assert!(
        !stack.contains("sha256"),
        "the observed process must not hash its own image: {stack}"
    );
    assert!(stack.contains("native_id="));
    assert!(
        std::fs::read_to_string(directory.join("panic-1.txt"))
            .unwrap()
            .contains("cannot unwind")
    );
    assert_dump(&directory.join("panic.dmp"), None);
    // The earlier panic must not use up the snapshot of the fatal one.
    assert_dump(&directory.join("panic-latest.dmp"), None);
    let exit = std::fs::read_to_string(directory.join("exit.txt")).unwrap();
    assert!(exit.contains(&format!(
        "exit_code=0x{:08X}",
        status.code().unwrap() as u32
    )));
    assert!(exit.contains("exit_intent_recorded=false"));
    let image = storage::image_sha256(&std::env::current_exe().unwrap()).unwrap();
    assert!(
        exit.contains(&format!("sha256={image}\nexit_code")),
        "the observer must record the image hash: {exit}"
    );
}

#[test]
fn native_exception_preserves_fault_context_in_an_external_minidump() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "native");
    assert_eq!(status.code().unwrap() as u32, 0xE042_1001);
    assert_dump(&directory.join("native.dmp"), Some(0xE042_1001));
    assert!(
        std::fs::read_to_string(directory.join("exit.txt"))
            .unwrap()
            .contains("exit_code=0xE0421001")
    );
}

#[test]
fn observer_records_abrupt_exit_without_a_panic_hook() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "abrupt");
    assert_eq!(status.code(), Some(37));
    let exit = std::fs::read_to_string(directory.join("exit.txt")).unwrap();
    assert!(exit.contains("exit_code=0x00000025"));
    assert!(exit.contains("exit_intent_recorded=false"));
    assert!(!directory.join("panic-0.txt").exists());
}

#[test]
fn observer_survives_external_process_termination() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "terminate");
    assert!(!status.success());
    let exit = std::fs::read_to_string(directory.join("exit.txt")).unwrap();
    assert!(exit.contains(&format!(
        "exit_code=0x{:08X}",
        status.code().unwrap() as u32
    )));
    assert!(exit.contains("exit_intent_recorded=false"));
}

#[test]
fn observer_records_a_renderer_whose_whole_job_is_terminated() {
    let root = TestRoot::new();
    let mut command = crate::process_job::HelperCommand::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "crash_report::tests::diagnostic_child",
            "--ignored",
            "--nocapture",
        ])
        .env("SGT_RUNTIME_STATE_ROOT", &root.0)
        .env("SGT_TEST_CRASH_MODE", "wait")
        .env_remove("RUST_BACKTRACE")
        .stdout_null()
        .stderr_null();
    let renderer = crate::overlay::compositor_process::spawn_renderer(&mut command, ()).unwrap();
    renderer.resume().unwrap();
    wait_until(|| root.0.join("probe-ready").exists());
    // Renderer retirement and kill-on-close tear down the whole job at once.
    renderer.terminate().unwrap();
    let directories: Vec<_> = std::fs::read_dir(root.reports())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), 1);
    wait_until(|| {
        std::fs::read_to_string(directories[0].join("exit.txt"))
            .is_ok_and(|text| text.contains("exit_intent_recorded=false"))
    });
    drop(renderer);
}

#[test]
fn intentional_exit_is_distinguishable_from_unexplained_exit() {
    let root = TestRoot::new();
    let (status, directory) = run_probe(&root, "quit");
    assert!(status.success());
    assert!(
        std::fs::read_to_string(directory.join("exit.txt"))
            .unwrap()
            .contains("exit_intent_recorded=true")
    );
    assert!(
        std::fs::read_to_string(directory.join("exit-intent.txt"))
            .unwrap()
            .contains("reason=quit")
    );
}

fn assert_dump(path: &Path, exception: Option<u32>) {
    let data = std::fs::read(path).unwrap();
    assert_eq!(&data[..4], b"MDMP");
    let u32_at = |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let count = u32_at(8) as usize;
    let directory = u32_at(12) as usize;
    if let Some(expected) = exception {
        let stream = (0..count)
            .map(|index| directory + index * 12)
            .find(|offset| u32_at(*offset) == 6)
            .expect("minidump must contain an exception stream");
        let rva = u32_at(stream + 8) as usize;
        assert_eq!(u32_at(rva + 8), expected);
    }
}

#[test]
fn retention_preserves_live_sessions_and_unfamiliar_data() {
    let root = TestRoot::new();
    let reports = root.reports();
    std::fs::create_dir_all(&reports).unwrap();
    let mut identity = storage::Identity::current().unwrap();
    let live = reports.join(identity.session());
    std::fs::create_dir(&live).unwrap();
    storage::write_report(
        &live.join("identity.json"),
        &serde_json::to_string(&identity).unwrap(),
    )
    .unwrap();
    let unknown = reports.join("user-data");
    std::fs::create_dir(&unknown).unwrap();
    std::fs::write(unknown.join("keep.txt"), "keep").unwrap();
    identity.pid = u32::MAX;
    for number in 1..=12 {
        identity.created = number;
        let directory = reports.join(identity.session());
        std::fs::create_dir(&directory).unwrap();
        storage::write_report(
            &directory.join("identity.json"),
            &serde_json::to_string(&identity).unwrap(),
        )
        .unwrap();
        std::fs::write(directory.join("exit.txt"), "complete").unwrap();
        if number == 1 {
            std::fs::write(directory.join("unknown.bin"), "keep").unwrap();
        }
    }
    storage::prune(&reports).unwrap();
    assert!(live.join("identity.json").exists());
    assert!(unknown.join("keep.txt").exists());
    identity.created = 1;
    assert!(
        reports
            .join(identity.session())
            .join("unknown.bin")
            .exists()
    );
    identity.created = 2;
    assert!(!reports.join(identity.session()).exists());
    identity.created = 12;
    assert!(reports.join(identity.session()).exists());
    assert_eq!(std::fs::read_dir(reports).unwrap().count(), 11);
}

#[test]
fn routine_launches_do_not_evict_earlier_incident_evidence() {
    let root = TestRoot::new();
    let reports = root.reports();
    std::fs::create_dir_all(&reports).unwrap();
    let mut identity = storage::Identity::current().unwrap();
    identity.pid = u32::MAX;
    for number in 1..=13 {
        identity.created = number;
        let directory = reports.join(identity.session());
        std::fs::create_dir(&directory).unwrap();
        storage::write_report(
            &directory.join("identity.json"),
            &serde_json::to_string(&identity).unwrap(),
        )
        .unwrap();
        let status = if number == 1 {
            "exit_code=0x00000025\nexit_intent_recorded=false\n"
        } else {
            "exit_code=0x00000000\nexit_intent_recorded=true\n"
        };
        std::fs::write(directory.join("exit.txt"), status).unwrap();
    }
    storage::prune(&reports).unwrap();
    identity.created = 1;
    assert!(reports.join(identity.session()).join("exit.txt").exists());
    assert_eq!(std::fs::read_dir(reports).unwrap().count(), 9);
}

#[test]
fn renderer_teardowns_do_not_evict_an_earlier_host_incident() {
    let root = TestRoot::new();
    let reports = root.reports();
    std::fs::create_dir_all(&reports).unwrap();
    let mut identity = storage::Identity::current().unwrap();
    identity.pid = u32::MAX;
    for number in 1..=13 {
        identity.created = number;
        // The first session is a host crash; every later one is a renderer
        // killed with its job when the host quit.
        identity.role = if number == 1 {
            "host"
        } else {
            "result-compositor"
        }
        .to_owned();
        let directory = reports.join(identity.session());
        std::fs::create_dir(&directory).unwrap();
        storage::write_report(
            &directory.join("identity.json"),
            &serde_json::to_string(&identity).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("exit.txt"),
            "exit_code=0x00000001\nexit_intent_recorded=false\n",
        )
        .unwrap();
    }
    storage::prune(&reports).unwrap();
    identity.created = 1;
    assert!(reports.join(identity.session()).join("exit.txt").exists());
    assert_eq!(std::fs::read_dir(reports).unwrap().count(), 9);
}

#[test]
fn one_unremovable_session_does_not_stop_retention() {
    use std::os::windows::fs::OpenOptionsExt as _;

    let root = TestRoot::new();
    let reports = root.reports();
    std::fs::create_dir_all(&reports).unwrap();
    let mut identity = storage::Identity::current().unwrap();
    identity.pid = u32::MAX;
    for number in 1..=12 {
        identity.created = number;
        let directory = reports.join(identity.session());
        std::fs::create_dir(&directory).unwrap();
        storage::write_report(
            &directory.join("identity.json"),
            &serde_json::to_string(&identity).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("exit.txt"),
            "exit_code=0x00000000\nexit_intent_recorded=true\n",
        )
        .unwrap();
        if number == 4 {
            std::fs::write(directory.join("snapshot.txt"), "held").unwrap();
        }
    }
    identity.created = 4;
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(reports.join(identity.session()).join("snapshot.txt"))
        .unwrap();
    let bootstrap = reports.join("bootstrap-4242");
    std::fs::create_dir(&bootstrap).unwrap();
    std::fs::write(bootstrap.join("events.log"), "early event\n").unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS opens a directory.
        .open(&bootstrap)
        .unwrap()
        .set_modified(old)
        .unwrap();

    assert!(
        storage::prune(&reports).is_err(),
        "the held session is reported"
    );
    drop(held);
    for created in 1..=3 {
        identity.created = created;
        assert!(
            !reports.join(identity.session()).exists(),
            "session {created}"
        );
    }
    identity.created = 4;
    for kept in ["identity.json", "exit.txt"] {
        assert!(
            reports.join(identity.session()).join(kept).exists(),
            "a partly removed session keeps {kept} for the next pass"
        );
    }
    assert!(!bootstrap.exists(), "stale bootstrap logs age out");

    storage::prune(&reports).unwrap();
    assert!(!reports.join(identity.session()).exists());
}

#[test]
fn oversized_reports_remain_valid_utf8_and_bounded() {
    let root = TestRoot::new();
    let path = root.0.join("bounded.txt");
    storage::write_report(&path, &"한".repeat(100_000)).unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() <= 256 * 1024);
    assert!(text.ends_with('한'));
}

#[test]
fn event_log_is_synchronous_and_retains_bounded_recent_complete_lines() {
    let root = TestRoot::new();
    for number in 0..20 {
        storage::append_event(
            &root.0,
            &format!("event-{number:02}: {}", "한".repeat(2_000)),
        )
        .unwrap();
    }
    let content = std::fs::read_to_string(root.0.join("events.log")).unwrap();
    assert!(content.len() <= 64 * 1024);
    assert!(content.contains("event-19:"));
    assert!(!content.contains("event-00:"));
    assert!(content.ends_with('\n'));
}

#[test]
fn concurrent_events_are_all_recorded() {
    let root = TestRoot::new();
    let directory = root.0.clone();
    let writers: Vec<_> = (0..8)
        .map(|writer| {
            let directory = directory.clone();
            std::thread::spawn(move || {
                for event in 0..5 {
                    storage::append_event(&directory, &format!("writer-{writer}-event-{event}"))
                        .unwrap();
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let content = std::fs::read_to_string(root.0.join("events.log")).unwrap();
    for writer in 0..8 {
        for event in 0..5 {
            assert!(content.contains(&format!("writer-{writer}-event-{event}\n")));
        }
    }
}

#[test]
#[ignore = "isolated subprocess entry"]
fn diagnostic_child() {
    let Ok(mode) = std::env::var("SGT_TEST_CRASH_MODE") else {
        return;
    };
    use windows::Win32::System::Diagnostics::Debug::{
        RaiseException, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode,
    };
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    }
    install();
    assert!(REPORTER.set(prepare().unwrap()).is_ok());
    unsafe {
        SetUnhandledExceptionFilter(Some(native_exception));
    }
    std::fs::write(
        std::env::var_os("SGT_RUNTIME_STATE_ROOT")
            .map(PathBuf::from)
            .unwrap()
            .join("probe-ready"),
        "ready",
    )
    .unwrap();
    match mode.as_str() {
        "caught-panic" => {
            let caught = std::panic::catch_unwind(|| panic!("caught-origin"));
            assert!(caught.is_err());
        }
        "callback-panic" => {
            extern "C" fn callback_origin() {
                panic!("callback-origin");
            }
            callback_origin();
        }
        "native" => unsafe {
            RaiseException(0xE042_1001, 1, None);
        },
        "abrupt" => std::process::exit(37),
        "wait" => loop {
            std::thread::park();
        },
        "quit" => exit(0, "quit"),
        _ => panic!("unknown diagnostic test mode"),
    }
}

#[test]
#[ignore = "isolated observer entry"]
fn monitor_child() {
    if let Ok(session) = std::env::var("SGT_TEST_MONITOR_SESSION") {
        monitor::run(&session).unwrap();
    }
}

#[test]
fn a_snapshot_request_is_fixed_width_and_round_trips() {
    for (thread, exception) in [(0, 0), (7, 0x1234_5678), (u32::MAX, usize::MAX)] {
        let request = monitor::encode_request(thread, exception);
        assert_eq!(request.len(), monitor::REQUEST_LEN);
        let text = std::str::from_utf8(&request).unwrap();
        assert_eq!(monitor::parse_request(text).unwrap(), (thread, exception));
    }
}

#[test]
fn legacy_full_memory_dumps_are_removed_once_found() {
    let root = TestRoot::new();
    let legacy = root.0.join("crash-dumps");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("old.dmp"), b"dump").unwrap();
    assert!(storage::remove_legacy_dump_directory(&root.0).unwrap());
    assert!(!legacy.exists());
    assert!(!storage::remove_legacy_dump_directory(&root.0).unwrap());
}
