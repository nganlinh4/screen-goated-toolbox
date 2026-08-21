use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::catalog::validate_identifier;
use super::lease::reserve_removal;
use super::receipt::{ComponentReceipt, RECEIPT_NAME, is_reparse_point, resolve_owned_path};

static REMOVAL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
const MAX_EMPTY_DIRECTORY_SCAN: usize = 4_096;
const MAX_EMPTY_DIRECTORY_DEPTH: usize = 32;
const REMOVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const REMOVAL_WAIT_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RemovalOutcome {
    Missing,
    Removed,
    Pending,
    RequiredBy(Vec<String>),
    PreservedModified(Vec<PathBuf>),
}

pub(crate) fn request_remove(id: &str) -> Result<RemovalOutcome> {
    request_remove_inner(id, true)
}

pub(crate) fn request_remove_and_wait(id: &str) -> Result<RemovalOutcome> {
    let outcome = request_remove(id)?;
    if !matches!(outcome, RemovalOutcome::Pending) {
        return Ok(outcome);
    }
    let deadline = Instant::now() + REMOVAL_WAIT_TIMEOUT;
    while super::lease::removal_pending(id) {
        if Instant::now() >= deadline {
            bail!("component {id} did not release its active use before removal timed out");
        }
        std::thread::sleep(REMOVAL_WAIT_INTERVAL);
    }
    request_remove_inner(id, false)
}

fn request_remove_inner(id: &str, resume_after_removal: bool) -> Result<RemovalOutcome> {
    let _mutation = super::acquire_mutation_guard()?;
    validate_identifier(id)?;
    if !super::embedded_catalog().is_removable(id) {
        bail!("component is not removable by this host");
    }
    super::pending::record(id)?;
    if reserve_removal(id) {
        return Ok(RemovalOutcome::Pending);
    }
    let outcome = run_reserved_removal(id)?;
    drop(_mutation);
    if resume_after_removal && matches!(outcome, RemovalOutcome::Missing | RemovalOutcome::Removed)
    {
        let _ = resume_pending();
    }
    Ok(outcome)
}

pub(super) fn run_reserved_removal(id: &str) -> Result<RemovalOutcome> {
    let _mutation = super::acquire_mutation_guard()?;
    let result = {
        let _guard = REMOVAL_LOCK
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        remove_now(id)
            .and_then(|outcome| super::models::finish_auxiliary_removal(id, outcome, &_mutation))
            .and_then(|outcome| {
                finish_pending(id, &outcome)?;
                Ok(outcome)
            })
    };
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            super::lease::finish_removal(id, false);
            return Err(error);
        }
    };
    let finished = matches!(
        outcome,
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::PreservedModified(_)
    );
    super::models::invalidate_status(id);
    super::lease::finish_removal(id, finished);
    Ok(outcome)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
fn lock_removal_filesystem() -> std::sync::MutexGuard<'static, ()> {
    REMOVAL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner())
}

pub(super) fn finish_pending(id: &str, outcome: &RemovalOutcome) -> Result<()> {
    if matches!(
        outcome,
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::PreservedModified(_)
    ) {
        super::pending::clear(id)?;
    }
    Ok(())
}

pub(super) fn resume_pending() -> Result<Vec<(String, RemovalOutcome)>> {
    let mut remaining = super::pending::list()?;
    let mut outcomes = Vec::new();
    while !remaining.is_empty() {
        let mut blocked = Vec::new();
        let mut progress = false;
        for id in remaining {
            let outcome = request_remove_and_wait(&id)?;
            if matches!(outcome, RemovalOutcome::RequiredBy(_)) {
                blocked.push(id);
            } else {
                outcomes.push((id, outcome));
                progress = true;
            }
        }
        if !progress {
            for id in blocked {
                outcomes.push((id.clone(), request_remove_and_wait(&id)?));
            }
            break;
        }
        remaining = blocked;
    }
    Ok(outcomes)
}

pub(super) fn remove_now(id: &str) -> Result<RemovalOutcome> {
    validate_identifier(id)?;
    if !super::embedded_catalog().is_removable(id) {
        bail!("component is not removable by this host");
    }
    let dependents = installed_dependents(id)?;
    if !dependents.is_empty() {
        return Ok(RemovalOutcome::RequiredBy(dependents));
    }
    let component_root = super::components_root().join(id);
    let Ok(metadata) = std::fs::symlink_metadata(&component_root) else {
        return Ok(RemovalOutcome::Missing);
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component root is not a regular directory");
    }
    let mut preserved = Vec::new();
    for version in bounded_directories(&component_root, 32)? {
        remove_version(&version, id, &mut preserved)?;
    }
    if preserved.is_empty() {
        let _ = std::fs::remove_dir(&component_root);
        Ok(RemovalOutcome::Removed)
    } else {
        Ok(RemovalOutcome::PreservedModified(preserved))
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn clean_all() -> Result<Vec<(String, RemovalOutcome)>> {
    let root = super::components_root();
    let Ok(metadata) = std::fs::symlink_metadata(&root) else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component registry root is not a regular directory");
    }
    let mut remaining = Vec::new();
    for component in bounded_directories(&root, 256)? {
        let id = component
            .file_name()
            .and_then(|name| name.to_str())
            .context("component directory name is invalid")?;
        validate_identifier(id)?;
        remaining.push(id.to_string());
    }
    let mut outcomes = Vec::new();
    while !remaining.is_empty() {
        let mut blocked = Vec::new();
        let mut progress = false;
        for id in remaining {
            let outcome = match request_remove_inner(&id, false) {
                Ok(outcome) => outcome,
                Err(error) => {
                    let path = root.join(&id);
                    crate::log_info!(
                        "[Components] could not remove {} during Clean All: {error:#}",
                        id
                    );
                    outcomes.push((id, RemovalOutcome::PreservedModified(vec![path])));
                    progress = true;
                    continue;
                }
            };
            if matches!(outcome, RemovalOutcome::RequiredBy(_)) {
                blocked.push(id);
            } else {
                outcomes.push((id, outcome));
                progress = true;
            }
        }
        if !progress {
            for id in blocked {
                let outcome = request_remove_inner(&id, false).unwrap_or_else(|error| {
                    crate::log_info!(
                        "[Components] could not remove {} during Clean All: {error:#}",
                        id
                    );
                    RemovalOutcome::PreservedModified(vec![root.join(&id)])
                });
                outcomes.push((id, outcome));
            }
            break;
        }
        remaining = blocked;
    }
    Ok(outcomes)
}

fn installed_dependents(id: &str) -> Result<Vec<String>> {
    let root = super::components_root();
    let Ok(metadata) = std::fs::symlink_metadata(&root) else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component registry root is not a regular directory");
    }
    let mut dependents = Vec::new();
    for component_root in bounded_directories(&root, 256)? {
        let component_id = component_root
            .file_name()
            .and_then(|name| name.to_str())
            .context("component directory name is invalid")?;
        validate_identifier(component_id)?;
        if component_id == id {
            continue;
        }
        for version_root in bounded_directories(&component_root, 32)? {
            let receipt = ComponentReceipt::read(&version_root.join(RECEIPT_NAME))?;
            if receipt.id != component_id
                || version_root.file_name().and_then(|name| name.to_str())
                    != Some(receipt.version.as_str())
            {
                bail!("component receipt does not match its directory");
            }
            if receipt
                .dependencies
                .iter()
                .any(|dependency| dependency == id)
                && !dependents.iter().any(|dependent| dependent == component_id)
            {
                dependents.push(component_id.to_string());
            }
        }
    }
    dependents.sort();
    Ok(dependents)
}

fn remove_version(
    version_root: &Path,
    expected_id: &str,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let metadata = std::fs::symlink_metadata(version_root)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component version root is not a regular directory");
    }
    let receipt_path = version_root.join(RECEIPT_NAME);
    let receipt = ComponentReceipt::read(&receipt_path)?;
    if receipt.id != expected_id {
        bail!("component receipt id does not match its directory");
    }
    if version_root.file_name().and_then(|name| name.to_str()) != Some(receipt.version.as_str()) {
        bail!("component receipt version does not match its directory");
    }
    let preserved_before = preserved.len();
    for owned in &receipt.files {
        let path = resolve_owned_path(version_root, &owned.path)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                std::fs::remove_file(&path)?;
                remove_empty_parents(path.parent(), version_root)?;
            }
            Ok(_) => preserved.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    prune_empty_directories(version_root, version_root, 0, &mut 0)?;
    for entry in std::fs::read_dir(version_root)? {
        let path = entry?.path();
        if path != receipt_path && !preserved.contains(&path) {
            preserved.push(path);
        }
    }
    if preserved.len() == preserved_before {
        std::fs::remove_file(receipt_path)?;
        let _ = std::fs::remove_dir(version_root);
    }
    Ok(())
}

fn prune_empty_directories(
    root: &Path,
    directory: &Path,
    depth: usize,
    visited: &mut usize,
) -> Result<()> {
    if depth > MAX_EMPTY_DIRECTORY_DEPTH {
        bail!("component directory tree is too deep");
    }
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            continue;
        }
        *visited += 1;
        if *visited > MAX_EMPTY_DIRECTORY_SCAN {
            bail!("component directory tree contains too many directories");
        }
        prune_empty_directories(root, &path, depth + 1, visited)?;
    }
    if directory != root {
        match std::fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn bounded_directories(root: &Path, maximum: usize) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(root)? {
        if result.len() >= maximum {
            bail!("component registry contains too many directories");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("component registry contains an unsafe entry");
        }
        result.push(path);
    }
    Ok(result)
}

fn remove_empty_parents(mut parent: Option<&Path>, stop: &Path) -> Result<()> {
    while let Some(directory) = parent {
        if directory == stop {
            break;
        }
        match std::fs::remove_dir(directory) {
            Ok(()) => parent = directory.parent(),
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn receipt_for(id: &str, bytes: &[u8]) -> ComponentReceipt {
        receipt_with_dependencies(id, bytes, Vec::new())
    }

    fn receipt_with_dependencies(
        id: &str,
        bytes: &[u8],
        dependencies: Vec<String>,
    ) -> ComponentReceipt {
        ComponentReceipt {
            schema_version: 1,
            id: id.to_string(),
            version: "1.0.0".to_string(),
            architecture: "x64".to_string(),
            dependencies,
            files: vec![super::super::OwnedComponentFile {
                path: "bin/tool.exe".into(),
                size_bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }],
        }
    }

    #[test]
    fn active_lease_defers_removal() {
        let id = "test-active-lease";
        let lease = super::super::acquire(id).unwrap();
        assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Pending);
        assert!(super::super::lease::pending(id));
        assert!(super::super::acquire(id).is_err());
        drop(lease);
        assert!(super::super::acquire(id).is_ok());
    }

    #[test]
    fn receipt_validation_rejects_parent_traversal() {
        let mut receipt = receipt_for("test-component", b"tool");
        receipt.files[0].path = "../tool.exe".into();
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn installed_dependent_must_be_removed_before_its_runtime() {
        let runtime_id = "test-runtime-dependency";
        let worker_id = "test-worker-dependent";
        let runtime_bytes = b"runtime";
        let worker_bytes = b"worker";

        let (runtime_root, worker_root) = {
            let _mutation = super::super::acquire_mutation_guard().unwrap();
            let _guard = lock_removal_filesystem();
            let runtime_root = super::super::ensure_version_root(runtime_id, "1.0.0").unwrap();
            let worker_root = super::super::ensure_version_root(worker_id, "1.0.0").unwrap();
            for (root, bytes) in [
                (runtime_root.as_path(), runtime_bytes.as_slice()),
                (worker_root.as_path(), worker_bytes.as_slice()),
            ] {
                std::fs::create_dir(root.join("bin")).unwrap();
                std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
            }
            super::super::write_receipt(&runtime_root, &receipt_for(runtime_id, runtime_bytes))
                .unwrap();
            super::super::write_receipt(
                &worker_root,
                &receipt_with_dependencies(worker_id, worker_bytes, vec![runtime_id.to_string()]),
            )
            .unwrap();
            (runtime_root, worker_root)
        };

        assert_eq!(
            request_remove(runtime_id).unwrap(),
            RemovalOutcome::RequiredBy(vec![worker_id.to_string()])
        );
        assert_eq!(request_remove(worker_id).unwrap(), RemovalOutcome::Removed);
        for _ in 0..100 {
            if !runtime_root.exists() {
                break;
            }
            let _ = request_remove(runtime_id);
            std::thread::yield_now();
        }
        assert!(!runtime_root.exists());
        assert_eq!(request_remove(runtime_id).unwrap(), RemovalOutcome::Missing);
        assert!(!worker_root.exists());
    }

    #[test]
    fn empty_runtime_directories_do_not_block_managed_removal() {
        let id = "test-empty-runtime-directories";
        let bytes = b"worker";
        let version_root = {
            let _mutation = super::super::acquire_mutation_guard().unwrap();
            let _guard = lock_removal_filesystem();
            let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
            std::fs::create_dir(root.join("bin")).unwrap();
            std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
            std::fs::create_dir_all(root.join("bin/runtime/vendor/logs")).unwrap();
            super::super::write_receipt(&root, &receipt_for(id, bytes)).unwrap();
            root
        };

        assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Removed);
        assert!(!version_root.exists());
    }

    #[test]
    fn unknown_runtime_files_are_still_preserved() {
        let id = "test-unknown-runtime-file";
        let bytes = b"worker";
        let unknown = {
            let _mutation = super::super::acquire_mutation_guard().unwrap();
            let _guard = lock_removal_filesystem();
            let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
            std::fs::create_dir(root.join("bin")).unwrap();
            std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
            let unknown = root.join("bin/runtime/output.log");
            std::fs::create_dir_all(unknown.parent().unwrap()).unwrap();
            std::fs::write(&unknown, b"runtime output").unwrap();
            super::super::write_receipt(&root, &receipt_for(id, bytes)).unwrap();
            unknown
        };

        let RemovalOutcome::PreservedModified(paths) = request_remove(id).unwrap() else {
            panic!("unknown runtime file should block managed removal");
        };
        assert!(unknown.exists());
        assert!(paths.iter().any(|path| unknown.starts_with(path)));
        std::fs::remove_file(&unknown).unwrap();
        assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Removed);
    }

    #[test]
    #[ignore = "mutates the isolated process component test root"]
    fn isolated_pending_replay_and_clean_all_delete_recorded_bytes_and_preserve_unknowns() {
        let pending_id = "test-pending-replay";
        let owned_id = "test-clean-all-owned";
        let preserved_id = "test-clean-all-preserved";
        let original = b"managed";

        let (pending_root, owned_root, preserved_root, modified, unknown) = {
            let _mutation = super::super::acquire_mutation_guard().unwrap();
            let _guard = lock_removal_filesystem();
            let mut roots = Vec::new();
            for id in [pending_id, owned_id, preserved_id] {
                let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
                std::fs::create_dir(root.join("bin")).unwrap();
                std::fs::write(root.join("bin/tool.exe"), original).unwrap();
                super::super::write_receipt(&root, &receipt_for(id, original)).unwrap();
                roots.push(root);
            }
            let modified = roots[2].join("bin/tool.exe");
            std::fs::write(&modified, b"user-modified").unwrap();
            let unknown = roots[2].join("notes.txt");
            std::fs::write(&unknown, b"user-owned").unwrap();
            (
                roots.remove(0),
                roots.remove(0),
                roots.remove(0),
                modified,
                unknown,
            )
        };

        super::super::pending::record(pending_id).unwrap();
        let replayed = resume_pending().unwrap();
        assert_eq!(
            replayed,
            vec![(pending_id.to_string(), RemovalOutcome::Removed)]
        );
        assert!(!pending_root.exists());
        assert!(
            !super::super::pending::list()
                .unwrap()
                .contains(&pending_id.to_string())
        );

        let outcomes = clean_all().unwrap();
        assert!(outcomes.contains(&(owned_id.to_string(), RemovalOutcome::Removed)));
        assert!(outcomes.iter().any(|(id, outcome)| {
            id == preserved_id && matches!(outcome, RemovalOutcome::PreservedModified(_))
        }));
        assert!(!owned_root.exists());
        assert!(preserved_root.exists());
        assert!(!modified.exists());
        assert_eq!(std::fs::read(&unknown).unwrap(), b"user-owned");

        std::fs::remove_file(unknown).unwrap();
        assert_eq!(
            request_remove(preserved_id).unwrap(),
            RemovalOutcome::Removed
        );
        assert!(!preserved_root.exists());
    }
}
