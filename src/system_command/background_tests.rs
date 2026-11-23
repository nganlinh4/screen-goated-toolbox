#[cfg(windows)]
#[test]
fn background_helpers_keep_pipes_arguments_environment_directory_and_exit_status() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use windows::Win32::System::Console::GetConsoleWindow;

    let test_name = format!(
        "{}::background_helpers_keep_pipes_arguments_environment_directory_and_exit_status",
        module_path!().split_once("::").unwrap().1
    );
    const CHILD: &str = "SGT_BACKGROUND_COMMAND_TEST";
    const VALUE: &str = "SGT_BACKGROUND_COMMAND_VALUE";
    const ARGUMENT: &str = "argument with spaces";
    if std::env::var(CHILD).as_deref() == Ok(test_name.as_str()) {
        assert!(unsafe { GetConsoleWindow() }.0.is_null());
        assert!(std::env::args().any(|argument| argument == ARGUMENT));
        assert_eq!(std::env::var(VALUE).unwrap(), "environment value");
        assert_eq!(std::env::current_dir().unwrap(), std::env::temp_dir());
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        assert_eq!(input, "pipe input");
        println!("background-helper-verified");
        eprintln!("background-helper-diagnostic");
        std::process::exit(7);
    }
    let mut child = super::background_command(std::env::current_exe().unwrap())
        .args(["--exact", &test_name, "--nocapture", ARGUMENT])
        .env(CHILD, &test_name)
        .env(VALUE, "environment value")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(b"pipe input").unwrap();
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("Owned background fixture did not exit within its deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .take(16 * 1024)
        .read_to_string(&mut output)
        .unwrap();
    let mut diagnostic = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .take(16 * 1024)
        .read_to_string(&mut diagnostic)
        .unwrap();
    assert_eq!(status.code(), Some(7), "{output}\n{diagnostic}");
    assert!(output.contains("background-helper-verified"));
    assert!(diagnostic.contains("background-helper-diagnostic"));
}
