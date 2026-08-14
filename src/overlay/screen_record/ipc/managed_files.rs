use std::fs;
use std::path::{Path, PathBuf};

pub fn delete_recording_file(path: &str) -> Result<(), String> {
    let local = crate::paths::app_local_data_dir();
    delete_confined_file(
        Path::new(path),
        &[
            local.join("recordings"),
            local.join("composition-snapshots"),
        ],
    )
}

fn delete_confined_file(path: &Path, approved_roots: &[PathBuf]) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Managed recording is unavailable: {error}"))?;
    if !metadata.is_file() || metadata_is_reparse(&metadata) {
        return Err("Refusing to delete a non-regular recording file".to_string());
    }
    let resolved = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve managed recording: {error}"))?;
    let approved = approved_roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|root| resolved.starts_with(&root) && resolved != root)
    });
    if !approved {
        return Err("Refusing to delete a file outside app-managed recordings".to_string());
    }
    fs::remove_file(&resolved)
        .map_err(|error| format!("Could not delete managed recording: {error}"))
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sgt-managed-recording-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn only_files_below_an_approved_root_can_be_deleted() {
        let root = fixture("confined");
        let recordings = root.join("recordings");
        let outside = root.join("exported.mp4");
        fs::create_dir_all(&recordings).unwrap();
        let managed = recordings.join("raw.mp4");
        fs::write(&managed, b"managed").unwrap();
        fs::write(&outside, b"user export").unwrap();

        delete_confined_file(&managed, std::slice::from_ref(&recordings)).unwrap();
        assert!(!managed.exists());
        assert!(delete_confined_file(&outside, &[recordings]).is_err());
        assert!(outside.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
