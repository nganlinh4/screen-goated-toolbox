//! Registry-wide startup reconciliation of interrupted one-shot installs.
//!
//! One-shot installers stage into `component-staging/{id}-{version}-{pid}-{seq}`
//! and download into `component-downloads/{id}-...-{pid}-{seq}.download` under
//! the runtime data root. A crash or power loss mid-install leaves those
//! multi-GB trees behind forever, so startup removes the ones whose owning
//! process is gone. External tools resume and reconcile themselves and are
//! deliberately not listed here.

use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

const MAX_PARENT_ENTRIES: usize = 512;
const MAX_TREE_ENTRIES: usize = 500_000;
const MAX_TREE_DEPTH: usize = 48;

/// Components whose installers stamp working paths with the owning PID and
/// never resume a previous attempt.
const ONE_SHOT_COMPONENT_IDS: [&str; 10] = [
    "creation-windows",
    "vc14-x64-runtime",
    "onnx-directml-runtime",
    "screen-text-detector",
    "screen-recorder",
    "recorder-web",
    "recorder-worker",
    "prompt-dj-web",
    "tts-playground-web",
    "computer-control-engine",
];

#[derive(Debug, Default, PartialEq, Eq)]
struct ReconcileReport {
    stages_removed: usize,
    downloads_removed: usize,
}

pub(crate) fn reconcile_interrupted_installs_in_background() {
    let spawned = std::thread::Builder::new()
        .name("sgt-component-reconcile".into())
        .spawn(|| {
            let result = super::acquire_mutation_guard().and_then(|_guard| {
                reconcile_working_root(
                    &crate::paths::app_runtime_local_data_dir(),
                    std::process::id(),
                    process_may_be_running,
                )
            });
            match result {
                Ok(report) if report != ReconcileReport::default() => crate::log_info!(
                    "[Components] Removed interrupted installs: stages={} downloads={}",
                    report.stages_removed,
                    report.downloads_removed
                ),
                Ok(_) => {}
                Err(error) => {
                    crate::log_info!("[Components] Interrupted-install maintenance failed: {error}")
                }
            }
        });
    if let Err(error) = spawned {
        crate::log_info!("[Components] Interrupted-install maintenance did not start: {error}");
    }
}

fn reconcile_working_root(
    root: &Path,
    current_pid: u32,
    may_be_running: impl Fn(u32) -> bool,
) -> Result<ReconcileReport> {
    let abandoned = |pid: u32| pid != current_pid && !may_be_running(pid);
    let mut report = ReconcileReport::default();
    for (path, metadata) in parent_entries(&root.join("component-staging"))? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if metadata.is_dir()
            && owner_pid(name, "").is_some_and(&abandoned)
            && remove_tree(&path).is_ok()
        {
            report.stages_removed += 1;
        }
    }
    for (path, metadata) in parent_entries(&root.join("component-downloads"))? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if metadata.is_file()
            && owner_pid(name, ".download").is_some_and(&abandoned)
            && std::fs::remove_file(&path).is_ok()
        {
            report.downloads_removed += 1;
        }
    }
    Ok(report)
}

/// Lists a working directory's direct entries, skipping reparse points.
fn parent_entries(parent: &Path) -> Result<Vec<(PathBuf, std::fs::Metadata)>> {
    let metadata = match std::fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || super::receipt::is_reparse_point(&metadata) {
        bail!(
            "component working directory is unsafe: {}",
            parent.display()
        );
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(parent)?.take(MAX_PARENT_ENTRIES) {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if !super::receipt::is_reparse_point(&metadata) {
            entries.push((path, metadata));
        }
    }
    Ok(entries)
}

/// Returns the owning PID of a one-shot working path named
/// `{id}-...-{pid}-{seq}{suffix}`, or `None` for anything else.
fn owner_pid(name: &str, suffix: &str) -> Option<u32> {
    let stem = name.strip_suffix(suffix)?;
    let id = ONE_SHOT_COMPONENT_IDS
        .iter()
        .filter(|id| stem.starts_with(&format!("{id}-")))
        .max_by_key(|id| id.len())?;
    let mut tail = stem[id.len() + 1..].rsplitn(3, '-');
    let sequence = tail.next()?;
    let pid = tail.next()?;
    let all_digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(sequence) || !all_digits(pid) {
        return None;
    }
    pid.parse().ok()
}

/// Removes a staging tree bottom-up without following reparse points. A tree
/// that contains one, or exceeds the depth/entry bounds, is left untouched.
pub(super) fn remove_tree(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_TREE_DEPTH {
            bail!("staging tree is too deep");
        }
        for entry in std::fs::read_dir(&directory)? {
            if files.len() + directories.len() > MAX_TREE_ENTRIES {
                bail!("staging tree has too many entries");
            }
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if super::receipt::is_reparse_point(&metadata) {
                bail!("staging tree contains a reparse point");
            }
            if metadata.is_dir() {
                directories.push(path.clone());
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                files.push(path);
            } else {
                bail!("staging tree contains an unsafe entry");
            }
        }
    }
    for file in files {
        std::fs::remove_file(file)?;
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        std::fs::remove_dir(directory)?;
    }
    Ok(())
}

/// True unless Windows reports that no process has this PID. Access denied
/// and any other uncertainty keep the staging tree.
fn process_may_be_running(pid: u32) -> bool {
    use windows::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    const E_INVALIDARG_FOR_MISSING_PID: u32 = 0x8007_0057;
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
    match unsafe { OpenProcess(access, false, pid) } {
        Ok(raw) => {
            let process = unsafe { OwnedHandle::from_raw_handle(raw.0) };
            let handle = HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(&process));
            let wait = unsafe { WaitForSingleObject(handle, 0) };
            wait == WAIT_TIMEOUT
        }
        Err(error) => error.code().0 as u32 != E_INVALIDARG_FOR_MISSING_PID,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sgt-interrupted-installs-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("component-staging")).unwrap();
        std::fs::create_dir_all(root.join("component-downloads")).unwrap();
        root
    }

    fn deep_stage(root: &Path, name: &str) -> PathBuf {
        let stage = root.join("component-staging").join(name);
        let mut leaf = stage.clone();
        for depth in 0..12 {
            leaf.push(format!("d{depth}"));
        }
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(leaf.join("python311.dll"), b"x").unwrap();
        std::fs::write(stage.join("receipt.json"), b"{}").unwrap();
        std::fs::create_dir_all(stage.join("empty")).unwrap();
        stage
    }

    #[test]
    fn crashed_one_shot_installs_are_removed_and_live_or_resumable_ones_kept() {
        let root = scratch("reconcile");
        let current = std::process::id();
        let crashed = deep_stage(&root, "onnx-directml-runtime-1.2.0-90001-0");
        let adopt = deep_stage(&root, "vc14-x64-runtime-adopt-90001-4");
        let running = deep_stage(&root, "screen-recorder-2.0.0-90002-1");
        let ours = deep_stage(&root, &format!("recorder-web-1.0-{current}-2"));
        let resumable = deep_stage(&root, "yt-dlp-x64-2026-10-29");
        let downloads = root.join("component-downloads");
        let stale_downloads = [
            "creation-windows-90001-3.download",
            "onnx-directml-runtime-1-90001-0.download",
        ];
        for name in stale_downloads {
            std::fs::write(downloads.join(name), b"partial").unwrap();
        }
        let foreign_download = downloads.join("yt-dlp-x64-90001-1.download");
        std::fs::write(&foreign_download, b"partial").unwrap();

        let report = reconcile_working_root(&root, current, |pid| pid == 90002).unwrap();

        assert_eq!(
            report,
            ReconcileReport {
                stages_removed: 2,
                downloads_removed: 2
            }
        );
        assert!(!crashed.exists() && !adopt.exists());
        for kept in [&running, &ours, &resumable, &foreign_download] {
            assert!(kept.exists(), "{}", kept.display());
        }
        for name in stale_downloads {
            assert!(!downloads.join(name).exists(), "{name}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn owner_pid_requires_a_listed_id_and_numeric_pid_and_sequence() {
        assert_eq!(owner_pid("recorder-worker-3.1-512-7", ""), Some(512));
        assert_eq!(
            owner_pid("recorder-web-512-0.download", ".download"),
            Some(512)
        );
        assert_eq!(owner_pid("recorder-web-1.0-abc-0", ""), None);
        assert_eq!(owner_pid("yt-dlp-x64-1.0-512-0", ""), None);
        assert_eq!(owner_pid("onnx-directml-runtime-512", ""), None);
        assert_eq!(owner_pid("recorder-web-1.0-512-0", ".download"), None);
    }

    #[test]
    fn listed_ids_exist_in_the_component_catalog() {
        let catalog = super::super::catalog::embedded_catalog();
        for id in ONE_SHOT_COMPONENT_IDS {
            assert!(
                catalog
                    .components
                    .iter()
                    .any(|component| component.id == id),
                "{id}"
            );
        }
    }

    #[test]
    fn missing_pid_is_not_running_but_this_process_is() {
        assert!(process_may_be_running(std::process::id()));
        // PIDs are multiples of four on Windows, so this one never exists.
        assert!(!process_may_be_running(u32::MAX - 2));
    }
}
