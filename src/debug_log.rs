use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

lazy_static::lazy_static! {
    static ref LOG_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

pub fn log_debug(msg: &str) {
    let _lock = LOG_MUTEX.lock().unwrap();
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("SGT");
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
            println!("{}", msg);
            $crate::debug_log::log_debug(&msg);
        }
    };
}
