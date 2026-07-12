use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::LazyLock;

static LOG_MUTEX: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));

pub fn print_line(msg: &str) {
    #[cfg(windows)]
    if write_console_line(msg) {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{msg}");
}

#[cfg(windows)]
fn write_console_line(msg: &str) -> bool {
    use windows::Win32::System::Console::{
        CONSOLE_MODE, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE, WriteConsoleW,
    };

    let Ok(handle) = (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) else {
        return false;
    };
    let mut mode = CONSOLE_MODE::default();
    if unsafe { GetConsoleMode(handle, &mut mode) }.is_err() {
        return false;
    }
    let wide: Vec<u16> = format!("{msg}\r\n").encode_utf16().collect();
    unsafe { WriteConsoleW(handle, &wide, None, None) }.is_ok()
}

pub fn log_debug(msg: &str) {
    let _lock = LOG_MUTEX.lock().unwrap();
    let mut path = crate::paths::app_sgt_dir();
    path.push("logs");
    let _ = std::fs::create_dir_all(&path);
    path.push("session.log");

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            $crate::debug_log::print_line(&msg);
            $crate::debug_log::log_debug(&msg);
        }
    };
}
