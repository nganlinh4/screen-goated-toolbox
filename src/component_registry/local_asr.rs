//! Downloadable Windows x64 ONNX/DirectML runtime.
//!
//! The module keeps its historical name because the signed delivery contract
//! (`windows-local-asr-v1`) and its packaging scripts are published under it.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Result, anyhow, bail};

use super::ComponentLease;
use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    is_reparse_point, resolve_owned_path,
};

#[cfg(not(feature = "recorder-worker"))]
mod control;
mod install;
mod staging;
#[cfg(not(feature = "recorder-worker"))]
mod update;
mod validation;

#[cfg(not(feature = "recorder-worker"))]
pub(crate) use control::{
    ComponentStatus, current_notice, current_status, remove, start_install, version_label,
};
use validation::{owned_file, validate_delivery, validate_exact_tree};

pub(crate) const RUNTIME_ID: &str = "onnx-directml-runtime";
const ARCHITECTURE: &str = "x64";
const VC_RUNTIME_ID: &str = "vc14-x64-runtime";
const MAX_COMPONENT_FILES: usize = 8;

#[derive(Clone, Copy)]
struct LocalAsrFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct LocalAsrDelivery {
    id: &'static str,
    version: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [LocalAsrFile],
}

include!(concat!(env!("OUT_DIR"), "/local_asr_delivery.rs"));

pub(crate) struct OnnxRuntimeUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl OnnxRuntimeUse {
    pub(crate) fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }
}

pub(crate) fn ensure_runtime(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<OnnxRuntimeUse> {
    let delivery = delivery(RUNTIME_ID)?;
    install::prefetch(delivery, cancelled, &on_progress)?;
    let _mutation = super::acquire_mutation_guard_cancellable(cancelled)?;
    install::ensure_delivery(delivery, cancelled, on_progress)?;
    validate_delivery(delivery)?;
    let root = version_root(RUNTIME_ID, delivery.version)?;
    let lease = super::acquire(RUNTIME_ID)?;
    let files = lock_component_files(&root, delivery.files)?;
    Ok(OnnxRuntimeUse {
        bin_dir: root.join("bin/x64"),
        _files: files,
        _lease: lease,
    })
}

fn delivery(id: &str) -> Result<&'static LocalAsrDelivery> {
    optional_delivery(id).ok_or_else(|| anyhow!("{id} download contract is unavailable"))
}

fn optional_delivery(id: &str) -> Option<&'static LocalAsrDelivery> {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.id == id)
    {
        return Some(delivery);
    }
    LOCAL_ASR_DELIVERIES
        .iter()
        .find(|delivery| delivery.id == id)
}

fn version_root(id: &str, version: &str) -> Result<PathBuf> {
    super::component_version_root(id, version)
}

fn lock_component_files(root: &Path, files: &[LocalAsrFile]) -> Result<Vec<std::fs::File>> {
    files
        .iter()
        .map(|expected| {
            super::native_file::lock_verified(
                root,
                expected.path,
                expected.size_bytes,
                expected.sha256,
                "native runtime component changed while acquiring its launch lease",
            )
        })
        .collect()
}

fn dependencies(id: &str) -> Result<Vec<String>> {
    match id {
        RUNTIME_ID => Ok(vec![VC_RUNTIME_ID.to_string()]),
        _ => bail!("unsupported native runtime component"),
    }
}

fn receipt(id: &str, version: &str, files: &[LocalAsrFile]) -> Result<ComponentReceipt> {
    Ok(ComponentReceipt {
        schema_version: 1,
        id: id.to_string(),
        version: version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: dependencies(id)?,
        files: files.iter().map(owned_file).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{RUNTIME_ID, optional_delivery};

    #[test]
    fn tracked_delivery_contains_the_runtime() {
        assert!(optional_delivery(RUNTIME_ID).is_some());
    }
}
