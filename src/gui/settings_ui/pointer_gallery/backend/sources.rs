use super::GalleryEvent;
use super::catalog::{CursorCollectionSpec, REQUIRED_FILES, source_name_for_file};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read as _, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(all(target_os = "windows", target_env = "msvc"))]
mod bsdtar;

const GITHUB_USER_AGENT: &str = "screen-goated-toolbox-pointer-gallery";
const MAX_CURSOR_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Deserialize)]
struct GithubContentEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
    download_url: Option<String>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "preload flow keeps progress, cancellation, and archive source inputs explicit"
)]
pub(super) fn preload_missing_from_github(
    spec: CursorCollectionSpec,
    local_dir: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
    files: &mut HashMap<String, PathBuf>,
    downloaded: &mut usize,
    total: usize,
    api_urls: &[&str],
) -> Result<(), String> {
    let mut remote_urls = HashMap::new();
    for api_url in api_urls {
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
        for entry in fetch_remote_entries(api_url)? {
            if entry.entry_type != "file" {
                continue;
            }
            if let Some(url) = entry.download_url {
                remote_urls.insert(entry.name.to_lowercase(), url);
            }
        }
    }

    for file_name in REQUIRED_FILES {
        let source_name = source_name_for_file(spec, file_name);
        if files.contains_key(file_name) {
            continue;
        }
        let source_key = source_name.to_lowercase();
        let Some(url) = remote_urls.get(&source_key) else {
            return Err(format!(
                "{} is missing required file '{}' for '{}'",
                spec.title, source_name, file_name
            ));
        };
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
        let local_path = local_dir.join(file_name);
        download_binary(url, &local_path)?;
        files.insert(file_name.to_string(), local_path.clone());
        *downloaded += 1;
        let _ = tx.send(GalleryEvent::FileReady {
            id: spec.id.to_string(),
            file_name: file_name.to_string(),
            path: local_path,
        });
        let _ = tx.send(GalleryEvent::Progress {
            id: spec.id.to_string(),
            downloaded: *downloaded,
            total,
        });
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "preload flow keeps progress, cancellation, and archive source inputs explicit"
)]
pub(super) fn preload_missing_from_zip(
    spec: CursorCollectionSpec,
    local_dir: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
    files: &mut HashMap<String, PathBuf>,
    downloaded: &mut usize,
    total: usize,
    archive_url: &str,
    subdir: &str,
) -> Result<(), String> {
    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    let archive_path = local_dir.join("_source.zip");
    if !archive_path.exists() {
        download_binary(archive_url, &archive_path)?;
    }
    validate_archive_file(&archive_path)?;

    let archive_file = fs::File::open(&archive_path)
        .map_err(|e| format!("Failed opening {:?}: {}", archive_path, e))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|e| format!("Failed reading zip for '{}': {}", spec.title, e))?;

    let mut entry_names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed listing zip entries for '{}': {}", spec.title, e))?;
        if entry.is_dir() {
            entry_names.push(String::new());
        } else {
            entry_names.push(normalize_archive_path(entry.name()));
        }
    }

    for file_name in REQUIRED_FILES {
        if files.contains_key(file_name) {
            continue;
        }
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }

        let source_name = source_name_for_file(spec, file_name);
        let Some(entry_index) = find_zip_entry_index(&entry_names, subdir, source_name) else {
            return Err(format!(
                "{} is missing required file '{}' for '{}'",
                spec.title, source_name, file_name
            ));
        };

        let local_path = local_dir.join(file_name);
        extract_zip_entry(&mut archive, entry_index, &local_path)?;
        files.insert(file_name.to_string(), local_path.clone());
        *downloaded += 1;
        let _ = tx.send(GalleryEvent::FileReady {
            id: spec.id.to_string(),
            file_name: file_name.to_string(),
            path: local_path,
        });
        let _ = tx.send(GalleryEvent::Progress {
            id: spec.id.to_string(),
            downloaded: *downloaded,
            total,
        });
    }

    Ok(())
}

fn fetch_remote_entries(api_url: &str) -> Result<Vec<GithubContentEntry>, String> {
    let response = crate::api::client::UREQ_AGENT
        .get(api_url)
        .header("User-Agent", GITHUB_USER_AGENT)
        .call()
        .map_err(|e| format!("Failed to query collection metadata: {}", e))?;
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read collection metadata: {}", e))?;
    serde_json::from_str::<Vec<GithubContentEntry>>(&body)
        .map_err(|e| format!("Failed to parse collection metadata: {}", e))
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
#[expect(
    clippy::too_many_arguments,
    reason = "RAR preload mirrors the shared preload contract even on the MSVC path"
)]
pub(super) fn preload_missing_from_rar(
    spec: CursorCollectionSpec,
    local_dir: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
    files: &mut HashMap<String, PathBuf>,
    downloaded: &mut usize,
    total: usize,
    archive_url: &str,
    subdir: &str,
) -> Result<(), String> {
    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    let archive_path = local_dir.join("_source.rar");
    if !archive_path.exists() {
        download_binary(archive_url, &archive_path)?;
    }
    validate_archive_file(&archive_path)?;

    let mut pending = REQUIRED_FILES
        .iter()
        .filter(|name| !files.contains_key(**name))
        .map(|name| PendingSource {
            local_name: (*name).to_string(),
            source_name: source_name_for_file(spec, name).to_string(),
            source_norm: normalize_archive_path(source_name_for_file(spec, name)),
        })
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return Ok(());
    }

    let subdir_norm = normalize_archive_path(subdir);
    let entries = bsdtar::list_entries(&archive_path, stop_signal)
        .map_err(|error| format!("Failed reading rar for '{}': {error}", spec.title))?;
    for entry_name in entries {
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }

        let entry_norm = normalize_archive_path(&entry_name);
        let matching_idx = pending.iter().position(|entry| {
            entry_matches_source(&entry_norm, &subdir_norm, entry.source_norm.as_str())
        });

        if let Some(idx) = matching_idx {
            let entry = pending.swap_remove(idx);
            let local_path = local_dir.join(&entry.local_name);
            bsdtar::extract_entry(&archive_path, &entry_name, &local_path, stop_signal).map_err(
                |error| {
                    format!(
                        "Failed extracting '{}' in '{}': {error}",
                        entry_name, spec.title
                    )
                },
            )?;

            files.insert(entry.local_name.clone(), local_path.clone());
            *downloaded += 1;
            let _ = tx.send(GalleryEvent::FileReady {
                id: spec.id.to_string(),
                file_name: entry.local_name,
                path: local_path,
            });
            let _ = tx.send(GalleryEvent::Progress {
                id: spec.id.to_string(),
                downloaded: *downloaded,
                total,
            });

            if pending.is_empty() {
                return Ok(());
            }
        }
    }

    if let Some(missing) = pending.first() {
        return Err(format!(
            "{} is missing required file '{}' for '{}'",
            spec.title, missing.source_name, missing.local_name
        ));
    }

    Ok(())
}

#[cfg(not(all(target_os = "windows", target_env = "msvc")))]
#[expect(
    clippy::too_many_arguments,
    reason = "stub keeps the same preload contract as the MSVC implementation"
)]
pub(super) fn preload_missing_from_rar(
    _spec: CursorCollectionSpec,
    _local_dir: &Path,
    _tx: &Sender<GalleryEvent>,
    _stop_signal: &AtomicBool,
    _files: &mut HashMap<String, PathBuf>,
    _downloaded: &mut usize,
    _total: usize,
    _archive_url: &str,
    _subdir: &str,
) -> Result<(), String> {
    Err("RAR extraction is supported only on Windows MSVC builds.".to_string())
}

fn find_zip_entry_index(entry_names: &[String], subdir: &str, source_name: &str) -> Option<usize> {
    let source_norm = normalize_archive_path(source_name);
    let subdir_norm = normalize_archive_path(subdir);

    if !subdir_norm.is_empty() {
        let direct = format!("{}/{}", subdir_norm, source_norm);
        if let Some(index) = entry_names
            .iter()
            .position(|name| name == &direct || name.ends_with(&format!("/{}", direct)))
        {
            return Some(index);
        }

        let scoped = format!("/{}/", subdir_norm);
        if let Some(index) = entry_names
            .iter()
            .position(|name| name.ends_with(&format!("/{}", source_norm)) && name.contains(&scoped))
        {
            return Some(index);
        }
    }

    entry_names
        .iter()
        .position(|name| name == &source_norm || name.ends_with(&format!("/{}", source_norm)))
}

fn extract_zip_entry<R: io::Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    destination: &Path,
) -> Result<(), String> {
    let temp_path = destination.with_extension("tmp");
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("Failed extracting zip entry {}: {}", index, e))?;
    let mut out = fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file {:?}: {}", temp_path, e))?;
    io::copy(&mut entry, &mut out)
        .map_err(|e| format!("Failed to write {:?}: {}", destination, e))?;
    out.flush()
        .map_err(|e| format!("Failed to flush {:?}: {}", destination, e))?;
    drop(out);

    fs::rename(&temp_path, destination).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Failed to move cursor file into place {:?}: {}",
            destination, e
        )
    })?;

    Ok(())
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
struct PendingSource {
    local_name: String,
    source_name: String,
    source_norm: String,
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
fn entry_matches_source(entry_norm: &str, subdir_norm: &str, source_norm: &str) -> bool {
    if !subdir_norm.is_empty() {
        let direct = format!("{}/{}", subdir_norm, source_norm);
        if entry_norm == direct || entry_norm.ends_with(&format!("/{}", direct)) {
            return true;
        }

        let scoped = format!("/{}/", subdir_norm);
        if entry_norm.ends_with(&format!("/{}", source_norm)) && entry_norm.contains(&scoped) {
            return true;
        }

        return false;
    }

    entry_norm == source_norm || entry_norm.ends_with(&format!("/{}", source_norm))
}

fn download_binary(url: &str, destination: &Path) -> Result<(), String> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", GITHUB_USER_AGENT)
        .call()
        .map_err(|e| format!("Failed to download {}: {}", url, e))?;
    let declared_bytes = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared_bytes.is_some_and(|bytes| bytes > MAX_CURSOR_SOURCE_BYTES) {
        return Err(format!("Cursor source from {} is larger than allowed", url));
    }

    let temp_path = destination.with_extension("tmp");
    prepare_download_temp(&temp_path)?;
    let mut reader = response.into_body().into_reader();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("Failed to create temp file {:?}: {}", temp_path, e))?;
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read {}: {}", url, e))?;
        if read == 0 {
            break;
        }
        downloaded = downloaded.saturating_add(read as u64);
        if downloaded > MAX_CURSOR_SOURCE_BYTES {
            drop(file);
            let _ = fs::remove_file(&temp_path);
            return Err(format!("Cursor source from {} is larger than allowed", url));
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("Failed to write {:?}: {}", destination, e))?;
    }
    file.flush()
        .map_err(|e| format!("Failed to flush {:?}: {}", destination, e))?;
    drop(file);

    fs::rename(&temp_path, destination).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Failed to move cursor file into place {:?}: {}",
            destination, e
        )
    })?;

    Ok(())
}

fn prepare_download_temp(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err(format!(
            "Cursor staging path {:?} is not a regular file",
            path
        ));
    }
    fs::remove_file(path).map_err(|error| format!("Failed to clear {:?}: {error}", path))
}

fn validate_archive_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Failed to inspect {:?}: {error}", path))?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err(format!("Cursor archive {:?} is not a regular file", path));
    }
    if metadata.len() > MAX_CURSOR_SOURCE_BYTES {
        return Err(format!("Cursor archive {:?} is larger than allowed", path));
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn normalize_archive_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_matching_is_scoped_and_suffix_tolerant() {
        assert!(entry_matches_source(
            "bundle/comix new black/arrow.cur",
            "comix new black",
            "arrow.cur",
        ));
        assert!(!entry_matches_source(
            "bundle/comix new white/arrow.cur",
            "comix new black",
            "arrow.cur",
        ));
    }
}
