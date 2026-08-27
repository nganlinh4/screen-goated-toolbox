//! Ownership and lifecycle boundary for optional downloaded components.

#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod capabilities;
mod catalog;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod computer_control;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod creation;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod external_tools;
mod lease;
pub(crate) mod local_asr;
pub(crate) mod models;
mod mutation;
mod pending;
pub(crate) mod qwen_runtime;
mod receipt;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod recorder;
mod removal;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod screen_text_detector;
#[cfg(not(feature = "recorder-worker"))]
mod staging;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod update_catalog;
pub(crate) mod vc_runtime;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod web_assets;

pub(crate) use catalog::embedded_catalog;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use catalog::validate_identifier;
pub(crate) use lease::{ComponentLease, acquire};
pub(crate) use mutation::{RegistryMutationGuard, acquire_mutation_guard};
pub(crate) use receipt::write_receipt;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use receipt::{ComponentReceipt, OwnedComponentFile};
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use removal::clean_all;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use removal::request_remove_and_wait;
pub(crate) use removal::{RemovalOutcome, request_remove};

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn resume_pending_removals() -> anyhow::Result<Vec<(String, RemovalOutcome)>> {
    removal::resume_pending()
}

pub(crate) fn components_root() -> std::path::PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!(
        "screen-goated-toolbox-component-tests-{}",
        std::process::id()
    ));
    #[cfg(not(test))]
    crate::paths::app_runtime_local_data_dir().join("components")
}

pub(crate) fn worker_workspace(id: &str) -> anyhow::Result<std::path::PathBuf> {
    catalog::validate_identifier(id)?;
    let root = crate::paths::app_runtime_local_data_dir().join("worker-workspaces");

    ensure_regular_directory(&root)?;
    let workspace = root.join(id);
    ensure_regular_directory(&workspace)?;
    let canonical = std::fs::canonicalize(&workspace)?;
    validate_regular_directory(&canonical)?;
    Ok(canonical)
}

pub(crate) fn component_version_root(
    id: &str,
    version: &str,
) -> anyhow::Result<std::path::PathBuf> {
    catalog::validate_identifier(id)?;
    catalog::validate_identifier(version)?;
    Ok(components_root().join(id).join(version))
}

pub(crate) fn ensure_component_parent(id: &str) -> anyhow::Result<std::path::PathBuf> {
    catalog::validate_identifier(id)?;
    let root = components_root();
    let app_root = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("component registry root has no parent"))?;
    std::fs::create_dir_all(app_root)?;
    validate_regular_directory(app_root)?;
    ensure_regular_directory(&root)?;
    let component = root.join(id);
    ensure_regular_directory(&component)?;
    Ok(component)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(crate) fn ensure_version_root(id: &str, version: &str) -> anyhow::Result<std::path::PathBuf> {
    catalog::validate_identifier(version)?;
    let version_root = ensure_component_parent(id)?.join(version);
    ensure_regular_directory(&version_root)?;
    Ok(version_root)
}

pub(crate) fn validate_version_root(id: &str, version: &str) -> anyhow::Result<std::path::PathBuf> {
    let version_root = component_version_root(id, version)?;
    validate_regular_directory(&version_root)?;
    Ok(version_root)
}

fn ensure_regular_directory(path: &std::path::Path) -> anyhow::Result<()> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(parent) = path.parent() else {
                return Err(error.into());
            };
            std::fs::create_dir_all(parent)?;
            match std::fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    }
    validate_regular_directory(path)
}

fn validate_regular_directory(path: &std::path::Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || receipt::is_reparse_point(&metadata) {
        anyhow::bail!("component registry path is not a regular directory");
    }
    Ok(())
}
