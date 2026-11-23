use super::*;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct ExecutableFixture {
    root: PathBuf,
    executable: PathBuf,
}
impl ExecutableFixture {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let root = std::env::temp_dir().join(format!("sgt-relaunch-{suffix}-é界 & % ! ( ) ^"));
        std::fs::create_dir(&root).unwrap();
        let executable = root.join("replacement é界 & % ! ( ) ^.exe");
        let original = std::env::current_exe().unwrap();
        if std::fs::hard_link(&original, &executable).is_err() {
            std::fs::copy(&original, &executable).unwrap();
        }
        Self { root, executable }
    }
}
impl Drop for ExecutableFixture {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!(
                "Relaunch fixture retained at {}: {error}",
                self.root.display()
            );
        }
    }
}
impl ExecutableFixture {
    fn cleanup(&self) -> std::io::Result<()> {
        if self.executable.exists() {
            std::fs::remove_file(&self.executable)?;
        }
        if self.root.exists() {
            std::fs::remove_dir(&self.root)?;
        }
        Ok(())
    }
}

#[test]
fn direct_relaunch_preserves_path_and_waits_for_exact_parent() {
    const ROLE: &str = "SGT_RELAUNCH_NATIVE_ROLE";
    const TARGET: &str = "SGT_RELAUNCH_NATIVE_TARGET";
    let name = format!(
        "{}::direct_relaunch_preserves_path_and_waits_for_exact_parent",
        module_path!().split_once("::").unwrap().1
    );
    match std::env::var(ROLE).as_deref() {
        Ok("replacement") => {
            println!("replacement-waiting");
            std::io::stdout().flush().unwrap();
            await_previous_process().unwrap();
            assert!(std::env::var_os(PARENT_MARKER).is_none());
            assert_eq!(
                std::env::current_exe().unwrap().canonicalize().unwrap(),
                PathBuf::from(std::env::var_os(TARGET).unwrap())
                    .canonicalize()
                    .unwrap()
            );
            assert_eq!(
                std::env::args_os().skip(1).collect::<Vec<_>>(),
                ["--exact", &name, "--nocapture"].map(std::ffi::OsString::from)
            );
            assert!(
                unsafe { windows::Win32::System::Console::GetConsoleWindow() }
                    .0
                    .is_null()
            );
            println!("replacement-ready");
            std::io::stdout().flush().unwrap();
            return;
        }
        Ok("parent") => {
            let target = PathBuf::from(std::env::var_os(TARGET).unwrap());
            let mut command =
                crate::runtime_support::restart_command(&target, ["--exact", &name, "--nocapture"])
                    .unwrap();
            command.env(ROLE, "replacement");
            let child = command.spawn().unwrap();
            println!("parent-ready:{}", child.id());
            // The outer fixture owns the job and verifies/reaps this replacement.
            let _replacement: OwnedHandle = child.into();
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_line(&mut String::new()).unwrap();
            return;
        }
        _ => {}
    }
    let fixture = ExecutableFixture::new();
    let mut command = crate::system_command::background_command(std::env::current_exe().unwrap());
    command
        .args(["--exact", &name, "--nocapture"])
        .env(ROLE, "parent")
        .env(TARGET, &fixture.executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The owned parent job also contains the replacement until both have exited.
    let mut parent = crate::process_job::OwnedProcess::spawn(&mut command, ()).unwrap();
    let stdout = parent.take_stdout().unwrap();
    let stderr = parent.take_stderr().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("Relaunch fixture: {line}");
        }
    });
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                return;
            }
        }
    });
    parent.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut pid = None;
    let mut waiting = false;
    while pid.is_none() || !waiting {
        let line = receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        assert!(
            !line.contains("replacement-ready"),
            "Replacement passed its parent before exit"
        );
        if let Some((_, value)) = line.split_once("parent-ready:") {
            pid = Some(value.trim().parse::<u32>().unwrap());
        }
        waiting |= line.contains("replacement-waiting");
    }
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid.unwrap(),
        )
    }
    .unwrap();
    let replacement = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    assert!(parent.try_wait().unwrap().is_none());
    let until = Instant::now() + Duration::from_millis(100);
    while let Ok(line) = receive.recv_timeout(until.saturating_duration_since(Instant::now())) {
        assert!(
            !line.contains("replacement-ready"),
            "Replacement must wait for the exact live parent"
        );
    }
    parent.take_stdin().unwrap().write_all(b"\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let line = receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if line.contains("replacement-ready") {
            break;
        }
    }
    assert_eq!(
        unsafe { WaitForSingleObject(HANDLE(replacement.as_raw_handle()), 5000) },
        WAIT_OBJECT_0
    );
    let mut code = 1;
    unsafe {
        windows::Win32::System::Threading::GetExitCodeProcess(
            HANDLE(replacement.as_raw_handle()),
            &mut code,
        )
    }
    .unwrap();
    assert_eq!(code, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = parent.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    parent.stop_within(Duration::from_secs(3)).unwrap();
    drop(replacement);
    drop(parent);
    fixture.cleanup().unwrap();
}
