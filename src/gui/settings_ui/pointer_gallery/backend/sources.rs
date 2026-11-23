use super::GalleryEvent;
use super::catalog::{
    CursorCollectionSpec, GithubSource, PinnedBytes, REQUIRED_FILES, source_name_for_file,
};
use crate::component_registry::is_reparse_point;
use pinned_download::{download_pinned, ensure_pinned_archive};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(all(target_os = "windows", target_env = "msvc"))]
mod bsdtar;
mod pinned_download;
#[cfg(windows)]
mod zip_extraction;

const MAX_CURSOR_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

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
    source: &GithubSource,
) -> Result<(), String> {
    for (index, file_name) in REQUIRED_FILES.iter().copied().enumerate() {
        if files.contains_key(file_name) {
            continue;
        }
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
        let url = source.file_url(index, source_name_for_file(spec, file_name));
        let local_path = local_dir.join(file_name);
        download_pinned(&url, &local_path, source.file_pin(index))?;
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
    pin: PinnedBytes,
) -> Result<(), String> {
    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    let archive_path = local_dir.join("_source.zip");
    ensure_pinned_archive(archive_url, &archive_path, pin)?;
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
            let name = entry
                .name()
                .map_err(|e| format!("Failed decoding zip entry for '{}': {}", spec.title, e))?;
            entry_names.push(normalize_archive_path(&name));
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
    pin: PinnedBytes,
) -> Result<(), String> {
    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    let archive_path = local_dir.join("_source.rar");
    ensure_pinned_archive(archive_url, &archive_path, pin)?;
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
    _pin: PinnedBytes,
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
    #[cfg(windows)]
    return zip_extraction::extract(archive, index, destination);
    #[cfg(not(windows))]
    {
        let _ = (archive, index, destination);
        Err("Cursor archive extraction is only supported on Windows".to_string())
    }
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
