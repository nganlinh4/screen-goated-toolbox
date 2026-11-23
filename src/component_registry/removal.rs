use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
#[cfg(any(test, not(feature = "recorder-worker")))]
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::catalog::validate_identifier;
use super::receipt::{ComponentReceipt, RECEIPT_NAME, is_reparse_point};

pub(super) mod attention;
mod guard;
#[cfg(all(test, not(feature = "recorder-worker")))]
mod pending_tests;
mod replay;
mod transaction;
use transaction::request_remove_with_policy;

static REMOVAL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
const MAX_EMPTY_DIRECTORY_SCAN: usize = 4_096;
const MAX_EMPTY_DIRECTORY_DEPTH: usize = 32;
#[cfg(any(test, not(feature = "recorder-worker")))]
const REMOVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(any(test, not(feature = "recorder-worker")))]
const REMOVAL_WAIT_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RemovalOutcome {
    Missing,
    Removed,
    Pending,
    RequiredBy(Vec<String>),
    PreservedModified(Vec<PathBuf>),
}

#[cfg(any(test, not(feature = "recorder-worker")))]
pub(crate) fn request_remove(id: &str) -> Result<RemovalOutcome> {
    request_remove_inner(id, true)
}

#[cfg(any(test, not(feature = "recorder-worker")))]
pub(crate) fn request_remove_and_wait(id: &str) -> Result<RemovalOutcome> {
    let deadline = Instant::now() + REMOVAL_WAIT_TIMEOUT;
    loop {
        let outcome = request_remove_inner(id, false)?;
        if !matches!(outcome, RemovalOutcome::Pending) {
            return Ok(outcome);
        }
        if Instant::now() >= deadline {
            bail!("component {id} did not release its active use before removal timed out");
        }
        std::thread::sleep(REMOVAL_WAIT_INTERVAL);
    }
}

#[cfg(any(test, not(feature = "recorder-worker")))]
pub(super) fn request_remove_inner(id: &str, resume_after_removal: bool) -> Result<RemovalOutcome> {
    request_remove_with_policy(id, resume_after_removal, false)
}

fn run_reserved_removal(id: &str, replay: bool) -> Result<RemovalOutcome> {
    let _mutation = super::acquire_mutation_guard()?;
    let result = {
        let _guard = REMOVAL_LOCK
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        remove_now(id)
            .inspect(|outcome| {
                if matches!(outcome, RemovalOutcome::Removed | RemovalOutcome::Missing) {
                    super::persisted_download::discard_leftovers(id);
                }
            })
            .and_then(|outcome| {
                if !replay || !matches!(outcome, RemovalOutcome::RequiredBy(_)) {
                    finish_pending(id, &outcome)?;
                }
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
    let finished = (!replay || !matches!(outcome, RemovalOutcome::RequiredBy(_)))
        && matches!(
            outcome,
            RemovalOutcome::Missing
                | RemovalOutcome::Removed
                | RemovalOutcome::RequiredBy(_)
                | RemovalOutcome::PreservedModified(_)
        );
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
    match outcome {
        RemovalOutcome::PreservedModified(_) => attention::set_preserved(id, true)?,
        RemovalOutcome::Missing | RemovalOutcome::Removed => attention::set_preserved(id, false)?,
        _ => {}
    }
    if matches!(
        outcome,
        RemovalOutcome::Missing
            | RemovalOutcome::Removed
            | RemovalOutcome::RequiredBy(_)
            | RemovalOutcome::PreservedModified(_)
    ) {
        super::pending::clear(id)?;
    }
    Ok(())
}

pub(super) fn resume_pending() -> Result<Vec<(String, RemovalOutcome)>> {
    replay::resume_pending()
}

pub(super) fn retry_pending_removal(id: &str) -> Result<Option<RemovalOutcome>> {
    replay::retry_pending_removal(id)
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
    let Some(metadata) = metadata_if_present(&component_root)? else {
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
        match guard::delete_directory(&component_root) {
            Ok(()) => Ok(RemovalOutcome::Removed),
            Err(_) if metadata_if_present(&component_root)?.is_none() => {
                Ok(RemovalOutcome::Removed)
            }
            Err(error) => Err(error),
        }
    } else {
        Ok(RemovalOutcome::PreservedModified(preserved))
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn clean_all(excluded: &[&str]) -> Result<Vec<(String, RemovalOutcome)>> {
    let root = super::components_root();
    let Some(metadata) = metadata_if_present(&root)? else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component registry root is not a regular directory");
    }
    let mut remaining = Vec::new();
    for component in bounded_directories(&root, 256)? {
        let Some(id) = component_directory_id(&component) else {
            continue;
        };
        if !excluded.contains(&id.as_str()) {
            remaining.push(id);
        }
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
    dependents_under(&super::components_root(), id)
}

/// Lists installed components that depend on `id`. A component whose own
/// state cannot be read only counts as a dependent when the catalog says it
/// may depend on `id`; it never blocks unrelated removals.
fn dependents_under(root: &Path, id: &str) -> Result<Vec<String>> {
    let Some(metadata) = metadata_if_present(root)? else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component registry root is not a regular directory");
    }
    let mut dependents = Vec::new();
    for component_root in bounded_directories(root, 256)? {
        let Some(component_id) = component_directory_id(&component_root) else {
            continue;
        };
        if component_id == id {
            continue;
        }
        let depends = installed_component_depends_on(&component_root, &component_id, id)
            .unwrap_or_else(|error| {
                crate::log_info!(
                    "[Components] could not read {component_id} while checking dependents of {id}: {error:#}"
                );
                catalog_declares_dependency(&component_id, id)
            });
        if depends {
            dependents.push(component_id);
        }
    }
    dependents.sort();
    Ok(dependents)
}

fn installed_component_depends_on(
    component_root: &Path,
    component_id: &str,
    id: &str,
) -> Result<bool> {
    let mut unreadable = None;
    for version_root in bounded_directories(component_root, 32)? {
        let receipt = match ComponentReceipt::read(&version_root.join(RECEIPT_NAME)) {
            Ok(receipt)
                if receipt.id == component_id
                    && version_root.file_name().and_then(|name| name.to_str())
                        == Some(receipt.version.as_str()) =>
            {
                receipt
            }
            Ok(_) => {
                unreadable = Some(anyhow::anyhow!(
                    "component receipt does not match its directory"
                ));
                continue;
            }
            Err(error) => {
                unreadable = Some(error);
                continue;
            }
        };
        // A readable version that needs `id` decides the answer even when a
        // sibling version is unreadable.
        if receipt
            .dependencies
            .iter()
            .any(|dependency| dependency == id)
        {
            return Ok(true);
        }
    }
    match unreadable {
        Some(error) => Err(error),
        None => Ok(false),
    }
}

fn catalog_declares_dependency(component_id: &str, id: &str) -> bool {
    super::embedded_catalog()
        .components
        .iter()
        .any(|component| {
            component.id == component_id
                && component
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == id)
        })
}

fn component_directory_id(component_root: &Path) -> Option<String> {
    let name = component_root.file_name().and_then(|name| name.to_str());
    match name.filter(|name| validate_identifier(name).is_ok()) {
        Some(name) => Some(name.to_string()),
        None => {
            crate::log_info!(
                "[Components] skipping unrecognized registry directory {}",
                component_root.display()
            );
            None
        }
    }
}

fn remove_version(
    version_root: &Path,
    expected_id: &str,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let root_guards = guard::lock_directory(version_root)?;
    let receipt_path = version_root.join(RECEIPT_NAME);
    let (receipt, receipt_handle, receipt_body) = guard::read_receipt(&receipt_path)?;
    if receipt.id != expected_id {
        bail!("component receipt id does not match its directory");
    }
    if version_root.file_name().and_then(|name| name.to_str()) != Some(receipt.version.as_str()) {
        bail!("component receipt version does not match its directory");
    }
    let preserved_before = preserved.len();
    for owned in &receipt.files {
        let path = version_root.join(&owned.path);
        match guard::delete_file(&path, guard::Expected::Owned(owned)) {
            Ok(true) => {
                remove_empty_parents(path.parent(), version_root)?;
            }
            Ok(false) => preserved.push(path),
            Err(error) if guard::is_io_error(&error) => return Err(error),
            Err(_) => preserved.push(path),
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
        drop(receipt_handle);
        if !guard::delete_file(&receipt_path, guard::Expected::Receipt(&receipt_body))? {
            preserved.push(receipt_path);
            return Ok(());
        }
        drop(root_guards);
        if let Err(error) = guard::delete_directory(version_root)
            && metadata_if_present(version_root)?.is_some()
        {
            return Err(error);
        }
    }
    Ok(())
}

fn metadata_if_present(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
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
    let directory_guards = guard::lock_directory(directory)?;
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
        drop(directory_guards);
        if let Err(error) = guard::delete_directory(directory)
            && !guard::is_directory_not_empty(&error)
            && metadata_if_present(directory)?.is_some()
        {
            return Err(error);
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
            crate::log_info!(
                "[Components] skipping non-directory registry entry {}",
                path.display()
            );
            continue;
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
        match guard::delete_directory(directory) {
            Ok(()) => parent = directory.parent(),
            Err(_) => break,
        }
    }
    Ok(())
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
