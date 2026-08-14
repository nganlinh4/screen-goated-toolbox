use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::security::MAX_BACKGROUND_BYTES;
use super::validate_background_id;

const BACKGROUND_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
static REPLACE_NONCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn backgrounds_dir() -> PathBuf {
    crate::paths::app_local_data_dir().join("backgrounds")
}

fn existing_background_paths(id: &str) -> Result<Vec<PathBuf>, String> {
    validate_background_id(id)?;
    let dir = backgrounds_dir();
    let mut paths = Vec::new();
    for ext in BACKGROUND_EXTENSIONS {
        let path = dir.join(format!("{id}.{ext}"));
        match std::fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata.file_type().is_symlink() =>
            {
                paths.push(path);
            }
            Ok(_) => {
                return Err(format!(
                    "Refusing to replace non-regular background {}",
                    path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Failed to inspect {}: {error}", path.display())),
        }
    }
    Ok(paths)
}

pub(super) fn delete_existing_files(id: &str) -> Result<(), String> {
    for path in existing_background_paths(id)? {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Failed to delete {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(super) fn publish_prepared_background(
    id: &str,
    prepared_path: &Path,
    prepared_ext: &str,
) -> Result<(), String> {
    let dir = backgrounds_dir();
    let final_path = dir.join(format!("{id}.{prepared_ext}"));
    let mut backups = Vec::new();
    for existing in existing_background_paths(id)? {
        let nonce = REPLACE_NONCE.fetch_add(1, Ordering::Relaxed);
        let backup = dir.join(format!(
            ".replace-{id}-{}-{}-{nonce}.bak",
            std::process::id(),
            backups.len()
        ));
        if let Err(error) = std::fs::rename(&existing, &backup) {
            for (original, moved) in backups.iter().rev() {
                let _ = std::fs::rename(moved, original);
            }
            return Err(format!(
                "Failed to stage {} for replacement: {error}",
                existing.display()
            ));
        }
        backups.push((existing, backup));
    }
    if let Err(error) = std::fs::rename(prepared_path, &final_path) {
        for (original, backup) in backups.iter().rev() {
            let _ = std::fs::rename(backup, original);
        }
        return Err(format!("Publish downloaded background failed: {error}"));
    }
    for (_, backup) in backups {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

pub(super) fn is_valid_image_file(path: &Path, ext: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() < 12
        || metadata.len() > MAX_BACKGROUND_BYTES
    {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut bytes = [0u8; 12];
    if file.read_exact(&mut bytes).is_err() {
        return false;
    }
    match ext {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8]),
        "webp" => bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP",
        _ => false,
    }
}
