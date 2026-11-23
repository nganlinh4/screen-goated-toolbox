//! The user's original cursor settings, saved once before the first pack is
//! applied. It is the only copy of those values and any existing file counts
//! as the backup, so it is replaced atomically and never left torn.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub(super) struct CursorBackup {
    pub(super) values: HashMap<String, String>,
    #[serde(default)]
    pub(super) cursor_base_size: Option<u32>,
}

pub(super) fn write(path: &Path, backup: &CursorBackup) -> Result<(), String> {
    crate::atomic_json::write_json_atomic(path, backup)
        .map_err(|error| format!("Failed writing backup file: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{CursorBackup, write};
    use std::collections::HashMap;

    #[test]
    fn failed_replacement_keeps_the_saved_originals() {
        let root = std::env::temp_dir().join(format!(
            "sgt-cursor-backup-{}-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("original-cursor-backup.json");
        let original = CursorBackup {
            values: HashMap::from([("Arrow".to_string(), "aero_arrow.cur".to_string())]),
            cursor_base_size: Some(32),
        };
        write(&path, &original).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let replacement = CursorBackup {
            values: HashMap::new(),
            cursor_base_size: None,
        };

        let result = crate::atomic_json::while_read_only(&path, || write(&path, &replacement));

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
