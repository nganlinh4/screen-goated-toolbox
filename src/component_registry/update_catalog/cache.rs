use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};

use super::{UpdateCatalog, parse_verified};

/// Catalogs kept after a successful store; older sequences are pruned.
const KEPT_CATALOGS: usize = 8;
/// Upper bound on directory entries examined, whatever their kind.
const MAX_SCANNED_ENTRIES: usize = 4_096;
static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) fn load_highest() -> Result<Option<UpdateCatalog>> {
    load_highest_in(&root())
}

/// Cached catalog files, newest sequence first. Signatures, temporaries and
/// foreign files never count toward the catalog window.
fn cached_catalogs(root: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let mut catalogs = Vec::new();
    for entry in std::fs::read_dir(root)?.take(MAX_SCANNED_ENTRIES) {
        let entry = entry?;
        let name = entry.file_name();
        let Some((sequence, _)) = name.to_str().and_then(super::network::parse_catalog_name) else {
            continue;
        };
        catalogs.push((sequence, entry.path()));
    }
    catalogs.sort_by(|left, right| right.cmp(left));
    Ok(catalogs)
}

fn load_highest_in(root: &Path) -> Result<Option<UpdateCatalog>> {
    let Ok(metadata) = std::fs::symlink_metadata(root) else {
        return Ok(None);
    };
    validate_directory(root, &metadata)?;
    let mut best = None;
    for (_, catalog_path) in cached_catalogs(root)? {
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
    if super::network::parse_catalog_name(name).is_none() {
        bail!("component catalog cache name is invalid");
    }
    let _guard = STORE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = ensure_root()?;
    store_in(&root, name, catalog, signature)
}

fn store_in(root: &Path, name: &str, catalog: &[u8], signature: &[u8]) -> Result<()> {
    let catalog_path = root.join(name);
    let signature_path = catalog_path.with_extension("sig");
    store_one(&catalog_path, catalog)?;
    store_one(&signature_path, signature)?;
    prune(root)
}

/// Keeps the newest catalog pairs so the cache stays small and offline
/// startup always sees the highest stored sequence.
fn prune(root: &Path) -> Result<()> {
    for (_, catalog_path) in cached_catalogs(root)?.into_iter().skip(KEPT_CATALOGS) {
        for path in [catalog_path.with_extension("sig"), catalog_path] {
            match std::fs::symlink_metadata(&path) {
                Ok(metadata)
                    if metadata.is_file()
                        && !super::super::receipt::is_reparse_point(&metadata) =>
                {
                    std::fs::remove_file(&path)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{KEPT_CATALOGS, cached_catalogs, store_in};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sgt-catalog-cache-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn name(sequence: u64) -> String {
        format!(
            "sgt-component-catalog-v{sequence:06}-{:016x}.json",
            sequence
        )
    }

    #[test]
    fn scan_counts_only_catalogs_and_orders_newest_first() {
        let root = temp_root("scan");
        for sequence in 1..=40 {
            std::fs::write(root.join(name(sequence)), b"{}").unwrap();
            std::fs::write(root.join(name(sequence)).with_extension("sig"), b"s").unwrap();
            std::fs::write(root.join(format!("{}.1.download", name(sequence))), b"t").unwrap();
        }
        let catalogs = cached_catalogs(&root).unwrap();
        assert_eq!(catalogs.len(), 40);
        assert_eq!(catalogs[0].0, 40);
        assert!(catalogs.windows(2).all(|pair| pair[0].0 > pair[1].0));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn store_prunes_older_pairs_and_keeps_the_newest() {
        let root = temp_root("prune");
        for sequence in 1..=(KEPT_CATALOGS as u64 + 5) {
            store_in(&root, &name(sequence), b"{}", b"s").unwrap();
        }
        let kept = cached_catalogs(&root).unwrap();
        assert_eq!(kept.len(), KEPT_CATALOGS);
        assert_eq!(kept[0].0, KEPT_CATALOGS as u64 + 5);
        assert!(!root.join(name(1)).exists());
        assert!(!root.join(name(1)).with_extension("sig").exists());
        assert!(
            root.join(name(KEPT_CATALOGS as u64 + 5))
                .with_extension("sig")
                .exists()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
