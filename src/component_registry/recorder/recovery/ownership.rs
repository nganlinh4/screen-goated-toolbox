use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::super::{RecorderDelivery, receipt as delivery_receipt};
use crate::component_registry::receipt::{ComponentReceipt, is_reparse_point};

pub(super) fn valid_receipt(path: &Path, delivery: &RecorderDelivery, snapshot_hash: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > 2 * 1024 * 1024 {
        return false;
    }
    let Ok(body) = std::fs::read(path) else {
        return false;
    };
    if format!("{:x}", Sha256::digest(&body)) != snapshot_hash {
        return false;
    }
    let Ok(receipt) = serde_json::from_slice::<ComponentReceipt>(&body) else {
        return false;
    };
    let expected = delivery_receipt(delivery);
    receipt.validate().is_ok()
        && receipt.id == expected.id
        && receipt.version == expected.version
        && receipt.architecture == expected.architecture
        && receipt.dependencies == expected.dependencies
        && receipt.files.len() == expected.files.len()
        && receipt
            .files
            .iter()
            .zip(&expected.files)
            .all(|(left, right)| {
                left.path == right.path
                    && left.size_bytes == right.size_bytes
                    && left.sha256.eq_ignore_ascii_case(&right.sha256)
            })
}
