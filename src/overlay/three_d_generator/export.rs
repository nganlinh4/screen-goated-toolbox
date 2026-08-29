use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest as _, Sha256};

const MAX_NAME_ATTEMPTS: usize = 1_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExportedResult {
    directory: String,
    paths: Vec<String>,
    names: Vec<String>,
}

pub(super) fn export_result(output_path: &str) -> Result<ExportedResult, String> {
    let entry = history_entry(output_path)?;
    validate_primary(&entry)?;
    let companion = crate::overlay::generation_history::validated_companion_path(&entry)?;
    let downloads = downloads_directory()?;
    let sources = std::iter::once(PathBuf::from(&entry.output_path))
        .chain(companion)
        .collect::<Vec<_>>();
    let exported = copy_group(&sources, &downloads)?;
    Ok(ExportedResult {
        directory: downloads.to_string_lossy().to_string(),
        names: exported
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect(),
        paths: exported
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
    })
}

fn history_entry(
    output_path: &str,
) -> Result<crate::overlay::generation_history::ResultHistoryEntry, String> {
    let requested = std::fs::canonicalize(output_path)
        .map_err(|_| "The selected project version is no longer available.".to_string())?;
    crate::overlay::generation_history::list("3d")?
        .into_iter()
        .find(|entry| std::fs::canonicalize(&entry.output_path).is_ok_and(|path| path == requested))
        .ok_or_else(|| "The selected project version is not in this project library.".to_string())
}

fn validate_primary(
    entry: &crate::overlay::generation_history::ResultHistoryEntry,
) -> Result<(), String> {
    super::asset_protocol::validate_glb(Path::new(&entry.output_path))?;
    let inspected =
        crate::overlay::generation_history::inspect_delivery_artifact(&entry.output_path)?;
    if inspected.size_bytes != entry.artifact_size_bytes
        || inspected.sha256 != entry.artifact_sha256
    {
        return Err("The selected project version changed before export.".to_string());
    }
    Ok(())
}

fn downloads_directory() -> Result<PathBuf, String> {
    let directory = crate::paths::user_downloads_dir()?;
    std::fs::create_dir_all(&directory)
        .map_err(|_| "The Downloads folder is unavailable.".to_string())?;
    std::fs::canonicalize(directory).map_err(|_| "The Downloads folder is unavailable.".to_string())
}

fn copy_group(sources: &[PathBuf], directory: &Path) -> Result<Vec<PathBuf>, String> {
    let primary = sources
        .first()
        .ok_or_else(|| "The selected project version is unavailable.".to_string())?;
    let stem = primary
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The project version name is invalid.".to_string())?;
    for attempt in 0..MAX_NAME_ATTEMPTS {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!(" ({})", attempt + 1)
        };
        let targets = sources
            .iter()
            .map(|source| {
                let extension = source
                    .extension()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "The project version format is invalid.".to_string())?;
                Ok(directory.join(format!("{stem}{suffix}.{extension}")))
            })
            .collect::<Result<Vec<_>, String>>()?;
        match reserve_targets(&targets) {
            Ok(mut files) => {
                if let Err(error) = write_targets(sources, &targets, &mut files) {
                    drop(files);
                    cleanup(&targets);
                    return Err(error);
                }
                return Ok(targets);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("The result could not be created in Downloads.".to_string()),
        }
    }
    Err("Downloads already contains too many versions with this name.".to_string())
}

fn reserve_targets(targets: &[PathBuf]) -> std::io::Result<Vec<File>> {
    let mut files = Vec::with_capacity(targets.len());
    for target in targets {
        match OpenOptions::new().write(true).create_new(true).open(target) {
            Ok(file) => files.push(file),
            Err(error) => {
                let created = files.len();
                drop(files);
                cleanup(&targets[..created]);
                return Err(error);
            }
        }
    }
    Ok(files)
}

fn write_targets(
    sources: &[PathBuf],
    targets: &[PathBuf],
    files: &mut [File],
) -> Result<(), String> {
    for ((source, target), destination) in sources.iter().zip(targets).zip(files) {
        let mut source_file = File::open(source)
            .map_err(|_| "The selected project version is unavailable.".to_string())?;
        let expected = source_file
            .metadata()
            .map_err(|_| "The selected project version is unavailable.".to_string())?
            .len();
        let copied = std::io::copy(&mut source_file, destination)
            .map_err(|_| "The result could not be written to Downloads.".to_string())?;
        destination
            .flush()
            .and_then(|_| destination.sync_all())
            .map_err(|_| "The result could not be finalized in Downloads.".to_string())?;
        if copied != expected || digest(source)? != digest(target)? {
            return Err("The exported result could not be verified.".to_string());
        }
    }
    Ok(())
}

fn digest(path: &Path) -> Result<[u8; 32], String> {
    let mut file =
        File::open(path).map_err(|_| "The exported result is unavailable.".to_string())?;
    file.rewind()
        .map_err(|_| "The exported result is unavailable.".to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "The exported result could not be verified.".to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_group_uses_one_collision_suffix_for_all_formats() {
        let root = std::env::temp_dir().join(format!(
            "sgt-3d-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let source = root.join("source");
        let destination = root.join("downloads");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let glb = source.join("robot.glb");
        let fbx = source.join("robot.fbx");
        std::fs::write(&glb, b"glb").unwrap();
        std::fs::write(&fbx, b"fbx").unwrap();
        std::fs::write(destination.join("robot.glb"), b"keep").unwrap();
        let copied = copy_group(&[glb, fbx], &destination).unwrap();
        assert_eq!(copied[0].file_name().unwrap(), "robot (2).glb");
        assert_eq!(copied[1].file_name().unwrap(), "robot (2).fbx");
        assert_eq!(
            std::fs::read(destination.join("robot.glb")).unwrap(),
            b"keep"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
