use std::fs;
use std::path::Path;

pub(super) fn get_dir_size(path: &Path) -> u64 {
    let mut total_size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    total_size += get_dir_size(&entry.path());
                } else {
                    total_size += metadata.len();
                }
            }
        }
    }
    total_size
}

pub(super) fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}
