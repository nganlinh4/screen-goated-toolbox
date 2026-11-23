use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn command(script: &str) -> HelperCommand {
    let shell = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])
    .unwrap();
    let mut command = HelperCommand::new(shell);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]);
    command
}
struct Resource(Arc<AtomicBool>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn diagnostic_capture_drains_large_pipes_and_preserves_exit_status() {
    let output = run(
        &command(
            "[Console]::Out.Write(('o'*200000)); [Console]::Error.Write(('e'*200000)); exit 7",
        ),
        (),
        || false,
        Duration::from_secs(15),
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, vec![b'o'; TAIL_BYTES]);
    assert_eq!(output.stderr, vec![b'e'; TAIL_BYTES]);
}

#[test]
fn cancellation_retires_the_owned_tree_and_its_resource() {
    let released = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let result = run(
        &command("Start-Sleep -Seconds 60"),
        Resource(released.clone()),
        || started.elapsed() > Duration::from_millis(250),
        Duration::from_secs(15),
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("cancelled"));
    assert!(released.load(Ordering::SeqCst));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_surviving_descendant_cannot_bypass_the_execution_deadline() {
    let script = r#"
$shell=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$leaf=Start-Process -FilePath $shell -WindowStyle Hidden -ArgumentList '-NoLogo','-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 60' -PassThru
exit 0
"#;
    let released = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let failure = run(
        &command(script),
        Resource(released.clone()),
        || false,
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(format!("{failure:#}").contains("timed out"));
    assert!(released.load(Ordering::SeqCst));
    assert!(started.elapsed() < Duration::from_secs(6));
}

#[test]
fn atomic_helper_preserves_native_arguments_null_input_and_hidden_console() {
    let name = format!(
        "{}::atomic_helper_preserves_native_arguments_null_input_and_hidden_console",
        module_path!().split_once("::").unwrap().1
    );
    let values = [
        "",
        "two words",
        "quote\"inside",
        "C:\\a b\\",
        "한글",
        "SGT_HELPER_ARGUMENT_FIXTURE",
    ];
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.last().map(String::as_str) == values.last().copied() {
        assert_eq!(&arguments[arguments.len() - values.len()..], values);
        assert!(
            unsafe { windows::Win32::System::Console::GetConsoleWindow() }
                .0
                .is_null()
        );
        let mut byte = [0];
        assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 0);
        println!("atomic-helper-arguments-ready");
        eprintln!("atomic-helper-error-ready");
        std::process::exit(7);
    }
    let mut command = HelperCommand::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", &name, "--nocapture", "--"])
        .args(values);
    let output = run(&command, (), || false, Duration::from_secs(15)).unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&output.stdout).contains("atomic-helper-arguments-ready"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("atomic-helper-error-ready"));
}
