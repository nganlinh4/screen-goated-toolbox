use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::LazyLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

static LOG_SENDER: LazyLock<Sender<String>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("sgt-log-writer".to_string())
        .spawn(move || writer_loop(receiver))
        .expect("failed to start SGT log writer");
    sender
});

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
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = LOG_SENDER.send(format!("[{timestamp}] {msg}"));
}

fn writer_loop(receiver: Receiver<String>) {
    let mut path = crate::paths::app_sgt_dir();
    path.push("logs");
    let _ = std::fs::create_dir_all(&path);
    path.push("session.log");

    let mut writer = open_writer(&path);
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                write_line(&path, &mut writer, &line);
                while let Ok(line) = receiver.try_recv() {
                    write_line(&path, &mut writer, &line);
                }
                if let Some(writer) = writer.as_mut() {
                    let _ = writer.flush();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(writer) = writer.as_mut() {
                    let _ = writer.flush();
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(writer) = writer.as_mut() {
                    let _ = writer.flush();
                }
                return;
            }
        }
    }
}

fn open_writer(path: &std::path::Path) -> Option<BufWriter<File>> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(BufWriter::new)
}

fn write_line(path: &std::path::Path, writer: &mut Option<BufWriter<File>>, line: &str) {
    if writer.is_none() {
        *writer = open_writer(path);
    }
    let failed = writer
        .as_mut()
        .is_none_or(|writer| writeln!(writer, "{line}").is_err());
    if failed {
        *writer = None;
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
