//! Stable output assignments for creation jobs.
//!
//! Every accepted dispatch receives one filename before it is persisted. The
//! assignment is reused during recovery and is never selected by probing for
//! the next available result name.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod sweep;
pub(crate) use sweep::sweep_staging;
#[cfg(test)]
use sweep::{sweep_staging_at, sweep_staging_bounded_at};

const MAX_VISIBLE_STEM_CHARACTERS: usize = 48;
const STAGING_MARKER_NAME: &str = ".sgt-staging.json";
const STAGING_MARKER_VERSION: u32 = 1;
const MAX_STAGING_MARKER_BYTES: u64 = 1_024;
const MAX_SCANNED_STAGING_ENTRIES: usize = 4_096;
const STAGING_ORPHAN_GRACE_MS: u64 = 60 * 60 * 1_000;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StagingMarker {
    version: u32,
    dispatch_id: String,
    output_name: String,
    created_at_ms: u64,
}

#[derive(Debug)]
pub(crate) struct StagingAssignment {
    directory: PathBuf,
    path: PathBuf,
    dispatch_id: String,
    output_name: String,
    armed: bool,
}

impl StagingAssignment {
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn persist(mut self) -> PathBuf {
        self.armed = false;
        self.directory.clone()
    }
}

impl Drop for StagingAssignment {
    fn drop(&mut self) {
        if self.armed {
            let _ = cleanup_staging(&self.dispatch_id, &self.output_name, &self.path);
        }
    }
}

fn valid_dispatch_id(dispatch_id: &str) -> bool {
    !dispatch_id.is_empty()
        && dispatch_id.len() <= 160
        && dispatch_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn assigned_name(
    source_path: &str,
    dispatch_id: &str,
    label: Option<&str>,
    extension: &str,
) -> Result<String, String> {
    if !valid_dispatch_id(dispatch_id)
        || extension.is_empty()
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || label.is_some_and(|value| {
            value.is_empty()
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
    {
        return Err("Creation output assignment is invalid.".to_string());
    }
    let visible_stem = Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Result")
        .chars()
        .filter_map(|value| {
            (!value.is_control()).then_some(if "<>:\"/\\|?*".contains(value) {
                '_'
            } else {
                value
            })
        })
        .take(MAX_VISIBLE_STEM_CHARACTERS)
        .collect::<String>();
    let visible_stem = visible_stem.trim().trim_end_matches(['.', ' ']);
    let visible_stem = if visible_stem.is_empty() {
        "Result"
    } else {
        visible_stem
    };
    let label = label.map(|value| format!("-{value}")).unwrap_or_default();
    Ok(format!(
        "{visible_stem}{label}-{dispatch_id}.{}",
        extension.to_ascii_lowercase()
    ))
}

pub(crate) fn assigned_path(output_dir: &Path, output_name: &str) -> Result<PathBuf, String> {
    let name = Path::new(output_name);
    if output_name.is_empty()
        || output_name.len() > 255
        || name.file_name() != Some(name.as_os_str())
        || output_name.ends_with(['.', ' '])
    {
        return Err("Creation output assignment is invalid.".to_string());
    }
    Ok(output_dir.join(name))
}

pub(crate) fn require_unoccupied(output_dir: &Path, output_name: &str) -> Result<PathBuf, String> {
    let path = assigned_path(output_dir, output_name)?;
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(_) => Err("Creation output destination is unavailable.".to_string()),
        Ok(_) => Err("Creation output destination is already in use.".to_string()),
    }
}

fn staging_root() -> Result<PathBuf, String> {
    let root = crate::paths::app_runtime_local_data_dir().join("creation-staging");
    std::fs::create_dir_all(&root).map_err(|_| "Creation staging is unavailable.".to_string())?;
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|_| "Creation staging is unavailable.".to_string())?;
    if !metadata.file_type().is_dir() || is_reparse_point(&metadata) {
        return Err("Creation staging is unavailable.".to_string());
    }
    std::fs::canonicalize(root).map_err(|_| "Creation staging is unavailable.".to_string())
}

pub(crate) fn prepare_staging(
    dispatch_id: &str,
    output_name: &str,
) -> Result<StagingAssignment, String> {
    if !valid_dispatch_id(dispatch_id) {
        return Err("Creation staging assignment is invalid.".to_string());
    }
    let root = staging_root()?;
    let directory = root.join(dispatch_id);
    std::fs::create_dir(&directory)
        .map_err(|_| "Creation staging assignment is already in use.".to_string())?;
    let directory = std::fs::canonicalize(&directory)
        .map_err(|_| "Creation staging is unavailable.".to_string())?;
    if directory.parent() != Some(root.as_path()) {
        let _ = std::fs::remove_dir(&directory);
        return Err("Creation staging assignment is invalid.".to_string());
    }
    let path = match require_unoccupied(&directory, output_name) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_dir(&directory);
            return Err(error);
        }
    };
    if let Err(error) = write_staging_marker(&directory, dispatch_id, output_name) {
        let _ = std::fs::remove_dir(&directory);
        return Err(error);
    }
    Ok(StagingAssignment {
        directory,
        path,
        dispatch_id: dispatch_id.to_string(),
        output_name: output_name.to_string(),
        armed: true,
    })
}

pub(crate) fn validate_staging_path(
    dispatch_id: &str,
    output_name: &str,
    path: &Path,
) -> Result<PathBuf, String> {
    if !valid_dispatch_id(dispatch_id) {
        return Err("Creation staging assignment is invalid.".to_string());
    }
    let root = staging_root()?;
    let directory = root.join(dispatch_id);
    let canonical_directory = match std::fs::canonicalize(&directory) {
        Ok(directory) if directory.parent() == Some(root.as_path()) => {
            validate_staging_marker(&directory, dispatch_id, output_name)?;
            directory
        }
        Ok(_) => return Err("Creation staging assignment is invalid.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => directory,
        Err(_) => return Err("Creation staging assignment is unavailable.".to_string()),
    };
    let expected = assigned_path(&canonical_directory, output_name)?;
    if expected != path {
        return Err("Creation staging assignment changed unexpectedly.".to_string());
    }
    Ok(expected)
}

pub(crate) fn staging_path(dispatch_id: &str, output_name: &str) -> Result<PathBuf, String> {
    if !valid_dispatch_id(dispatch_id) {
        return Err("Creation staging assignment is invalid.".to_string());
    }
    assigned_path(&staging_root()?.join(dispatch_id), output_name)
}

pub(crate) fn cleanup_staging(
    dispatch_id: &str,
    output_name: &str,
    path: &Path,
) -> Result<(), String> {
    let expected = validate_staging_path(dispatch_id, output_name, path)?;
    match std::fs::symlink_metadata(&expected) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            std::fs::remove_file(&expected)
                .map_err(|_| "Creation staging cleanup could not finish.".to_string())?;
        }
        Ok(_) => return Err("Creation staging cleanup refused an unsafe entry.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Creation staging cleanup could not inspect its entry.".to_string()),
    }
    let directory = expected
        .parent()
        .ok_or_else(|| "Creation staging assignment is invalid.".to_string())?;
    validate_staging_marker(directory, dispatch_id, output_name)?;
    let marker = directory.join(STAGING_MARKER_NAME);
    std::fs::remove_file(&marker)
        .map_err(|_| "Creation staging cleanup could not finish.".to_string())?;
    if std::fs::read_dir(directory)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(directory);
    }
    Ok(())
}

fn remove_empty_or_marker_only_directory(directory: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?
        .take(2)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?;
    if entries.len() > 1 {
        return Err("Creation staging cleanup found unexpected entries.".to_string());
    }
    if let Some(entry) = entries.first() {
        if entry.file_name() != STAGING_MARKER_NAME {
            return Err("Creation staging cleanup found an unowned entry.".to_string());
        }
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?;
        if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
            return Err("Creation staging cleanup refused an unsafe entry.".to_string());
        }
        std::fs::remove_file(entry.path())
            .map_err(|_| "Creation staging cleanup could not finish.".to_string())?;
    }
    std::fs::remove_dir(directory)
        .map_err(|_| "Creation staging cleanup could not finish.".to_string())
}

fn remove_owned_staging_directory(directory: &Path, marker: &StagingMarker) -> Result<(), String> {
    let expected = assigned_path(directory, &marker.output_name)?;
    let mut saw_marker = false;
    let mut scanned = 0_usize;
    for entry in std::fs::read_dir(directory)
        .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?
    {
        let entry = entry
            .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?;
        scanned += 1;
        if scanned > 2 {
            return Err("Creation staging cleanup found unexpected entries.".to_string());
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "Creation staging cleanup could not inspect its entry.".to_string())?;
        if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
            return Err("Creation staging cleanup refused an unsafe entry.".to_string());
        }
        if path == directory.join(STAGING_MARKER_NAME) {
            saw_marker = true;
        } else if path != expected {
            return Err("Creation staging cleanup found an unowned entry.".to_string());
        }
    }
    if !saw_marker {
        return Err("Creation staging ownership marker is missing.".to_string());
    }
    if expected.exists() {
        std::fs::remove_file(&expected)
            .map_err(|_| "Creation staging cleanup could not finish.".to_string())?;
    }
    std::fs::remove_file(directory.join(STAGING_MARKER_NAME))
        .map_err(|_| "Creation staging cleanup could not finish.".to_string())?;
    std::fs::remove_dir(directory)
        .map_err(|_| "Creation staging cleanup could not finish.".to_string())
}

fn write_staging_marker(
    directory: &Path,
    dispatch_id: &str,
    output_name: &str,
) -> Result<(), String> {
    let marker = StagingMarker {
        version: STAGING_MARKER_VERSION,
        dispatch_id: dispatch_id.to_string(),
        output_name: output_name.to_string(),
        created_at_ms: now_ms(),
    };
    let bytes = serde_json::to_vec(&marker)
        .map_err(|_| "Creation staging ownership could not be saved.".to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(STAGING_MARKER_NAME))
        .map_err(|_| "Creation staging ownership could not be saved.".to_string())?;
    use std::io::Write as _;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "Creation staging ownership could not be saved.".to_string())
}

fn validate_staging_marker(
    directory: &Path,
    dispatch_id: &str,
    output_name: &str,
) -> Result<(), String> {
    let marker = read_staging_marker(directory)?;
    if marker.version != STAGING_MARKER_VERSION
        || marker.dispatch_id != dispatch_id
        || marker.output_name != output_name
    {
        return Err("Creation staging ownership is invalid.".to_string());
    }
    Ok(())
}

fn read_staging_marker(directory: &Path) -> Result<StagingMarker, String> {
    use std::io::Read as _;
    let path = directory.join(STAGING_MARKER_NAME);
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "Creation staging ownership is unavailable.".to_string())?;
    if !metadata.file_type().is_file()
        || is_reparse_point(&metadata)
        || metadata.len() > MAX_STAGING_MARKER_BYTES
    {
        return Err("Creation staging ownership is invalid.".to_string());
    }
    let file = std::fs::File::open(&path)
        .map_err(|_| "Creation staging ownership is unavailable.".to_string())?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| "Creation staging ownership is unavailable.".to_string())?;
    if !opened_metadata.file_type().is_file()
        || opened_metadata.len() != metadata.len()
        || opened_metadata.len() > MAX_STAGING_MARKER_BYTES
    {
        return Err("Creation staging ownership is invalid.".to_string());
    }
    let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
    file.take(MAX_STAGING_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Creation staging ownership is invalid.".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Creation staging ownership is invalid.".to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_same_source_dispatches_get_distinct_stable_names() {
        let first = assigned_name(r"C:\input\same.png", "dispatch-100-1", None, "svg").unwrap();
        let second = assigned_name(r"C:\input\same.png", "dispatch-100-2", None, "svg").unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first,
            assigned_name(r"C:\input\same.png", "dispatch-100-1", None, "svg").unwrap()
        );
    }

    #[test]
    fn existing_destination_is_never_selected_for_a_new_dispatch() {
        let root = std::env::temp_dir().join(format!(
            "sgt-output-assignment-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let name = assigned_name("same.png", "dispatch-existing", None, "glb").unwrap();
        std::fs::write(root.join(&name), b"existing").unwrap();
        assert!(require_unoccupied(&root, &name).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn sweep_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sgt-staging-sweep-{label}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn sweep_converges_valid_malformed_and_markerless_crash_directories() {
        let root = sweep_root("crash-cuts");
        let valid = root.join("dispatch-valid");
        let malformed = root.join("dispatch-malformed");
        let markerless = root.join("dispatch-markerless");
        let unknown = root.join("dispatch-unknown");
        for directory in [&valid, &malformed, &markerless, &unknown] {
            std::fs::create_dir(directory).unwrap();
        }
        write_staging_marker(&valid, "dispatch-valid", "result.png").unwrap();
        std::fs::write(malformed.join(STAGING_MARKER_NAME), b"{partial").unwrap();
        std::fs::write(unknown.join("user-file.bin"), b"keep").unwrap();

        sweep_staging_at(&root, &Default::default(), now_ms(), 0).unwrap();

        assert!(!valid.exists());
        assert!(!malformed.exists());
        assert!(!markerless.exists());
        assert_eq!(
            std::fs::read(unknown.join("user-file.bin")).unwrap(),
            b"keep"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn protected_crash_directory_is_never_swept() {
        let root = sweep_root("protected");
        let directory = root.join("dispatch-protected");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join(STAGING_MARKER_NAME), b"{partial").unwrap();
        let protected = ["dispatch-protected".to_string()].into_iter().collect();
        sweep_staging_at(&root, &protected, now_ms(), 0).unwrap();
        assert!(directory.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_sweep_converges_across_an_orphan_storm() {
        let root = sweep_root("bound");
        let protected_id = "dispatch-protected-storm";
        for index in 0..25 {
            let dispatch = format!("dispatch-storm-{index:02}");
            let directory = root.join(&dispatch);
            std::fs::create_dir(&directory).unwrap();
            write_staging_marker(&directory, &dispatch, "result.png").unwrap();
        }
        let protected_directory = root.join(protected_id);
        std::fs::create_dir(&protected_directory).unwrap();
        write_staging_marker(&protected_directory, protected_id, "result.png").unwrap();
        let protected = [protected_id.to_string()].into_iter().collect();
        for _ in 0..30 {
            sweep_staging_bounded_at(&root, &protected, now_ms(), 0, 3).unwrap();
        }
        assert!(protected_directory.exists());
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .count(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
