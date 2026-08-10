use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};

use super::{UpdateCatalog, parse_verified};

const MAX_CACHED_CATALOGS: usize = 64;
static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) fn load_highest() -> Result<Option<UpdateCatalog>> {
    let root = root();
    let Ok(metadata) = std::fs::symlink_metadata(&root) else {
        return Ok(None);
    };
    validate_directory(&root, &metadata)?;
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&root)?.take(MAX_CACHED_CATALOGS + 1) {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with("sgt-component-catalog-v") && name.ends_with(".json") {
            candidates.push(entry.path());
        }
    }
    if candidates.len() > MAX_CACHED_CATALOGS {
        bail!("component catalog cache contains too many entries");
    }
    let mut best = None;
    for catalog_path in candidates {
        let signature_path = catalog_path.with_extension("sig");
        let Ok(catalog) = read_regular(&catalog_path, 2 * 1024 * 1024) else {
            continue;
        };
        let Ok(signature) = read_regular(&signature_path, 64) else {
            continue;
        };
        let Ok(parsed) = parse_verified(&catalog, &signature) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|current: &UpdateCatalog| parsed.sequence > current.sequence)
        {
            best = Some(parsed);
        }
    }
    Ok(best)
}

pub(super) fn store(name: &str, catalog: &[u8], signature: &[u8]) -> Result<()> {
    if !name.starts_with("sgt-component-catalog-v") || !name.ends_with(".json") {
        bail!("component catalog cache name is invalid");
    }
    let _guard = STORE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = ensure_root()?;
    let catalog_path = root.join(name);
    let signature_path = catalog_path.with_extension("sig");
    store_one(&catalog_path, catalog)?;
    store_one(&signature_path, signature)
}

fn root() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("update-catalog")
}

fn ensure_root() -> Result<PathBuf> {
    let root = root();
    let parent = root
        .parent()
        .context("component catalog cache has no parent")?;
    std::fs::create_dir_all(parent)?;
    match std::fs::create_dir(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(&root)?;
    validate_directory(&root, &metadata)?;
    Ok(root)
}

fn store_one(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if read_regular(path, bytes.len() as u64)? == bytes {
            return Ok(());
        }
        bail!("existing component catalog cache entry has different bytes");
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("component catalog cache filename is invalid")?;
    let temporary = path.with_file_name(format!("{file_name}.{}.download", std::process::id()));
    remove_stale_temporary(&temporary)?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    output.write_all(bytes)?;
    output.flush()?;
    output.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn remove_stale_temporary(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file() && !super::super::receipt::is_reparse_point(&metadata) =>
        {
            std::fs::remove_file(path)?;
        }
        Ok(_) => bail!("component catalog staging path is unsafe"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn read_regular(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || super::super::receipt::is_reparse_point(&metadata)
        || metadata.len() > maximum
    {
        bail!("component catalog cache entry is not a bounded regular file");
    }
    Ok(std::fs::read(path)?)
}

fn validate_directory(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(metadata) {
        bail!(
            "component catalog cache is not a regular directory: {}",
            path.display()
        );
    }
    Ok(())
}
