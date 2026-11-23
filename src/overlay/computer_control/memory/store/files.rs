//! Bounded store reads, corruption preservation and canonical owned filenames.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::super::{ConvMeta, Transcript};

pub(super) const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn regular_file(path: &Path) -> anyhow::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    ensure!(
        regular_file(path)?,
        "conversation memory requires regular files"
    );
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_JSON_BYTES as u64,
        "conversation memory file exceeds its read limit"
    );
    let mut bytes = Vec::new();
    file.take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_JSON_BYTES,
        "conversation memory file exceeds its read limit"
    );
    Ok(Some(bytes))
}

pub(super) fn read_index(path: &Path) -> anyhow::Result<Option<Vec<ConvMeta>>> {
    let Some(bytes) = read(path)? else {
        return Ok(None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(index) => Ok(Some(index)),
        Err(_) => {
            preserve_bytes(path, &bytes)?;
            Ok(None)
        }
    }
}

pub(super) fn read_record(path: &Path) -> anyhow::Result<Transcript> {
    serde_json::from_slice(&read(path)?.context("conversation record is missing")?)
        .context("conversation record is invalid")
}

pub(super) fn recover_record(path: &Path) -> anyhow::Result<Option<Transcript>> {
    let Some(bytes) = read(path)? else {
        return Ok(None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(record) => Ok(Some(record)),
        Err(_) => {
            preserve_bytes(path, &bytes)?;
            Ok(None)
        }
    }
}

pub(super) fn check_json(value: &impl Serialize) -> anyhow::Result<()> {
    ensure!(
        serde_json::to_vec_pretty(value)?.len() <= MAX_JSON_BYTES,
        "conversation record exceeds its write limit"
    );
    Ok(())
}

pub(super) fn write_record(path: &Path, record: &Transcript) -> anyhow::Result<()> {
    check_json(record)?;
    crate::atomic_json::write_json_atomic(path, record)?;
    Ok(())
}

pub(super) fn preserve(path: &Path) -> anyhow::Result<()> {
    preserve_bytes(
        path,
        &read(path)?.context("conversation recovery input is missing")?,
    )
}

fn preserve_bytes(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".corrupt-{digest}.json"));
    let backup = std::path::PathBuf::from(name);
    if let Some(existing) = read(&backup)? {
        ensure!(
            existing == bytes,
            "conversation recovery backup conflicts with its source"
        );
    } else {
        crate::atomic_json::write_bytes_atomic(&backup, bytes)?;
    }
    Ok(())
}

pub(super) fn record_id(name: &OsStr, prefix: &str) -> Option<i64> {
    let name = name.to_str()?;
    let id = name
        .strip_prefix(prefix)?
        .strip_suffix(".json")?
        .parse::<i64>()
        .ok()?;
    (name == format!("{prefix}{id}.json")).then_some(id)
}

pub(super) fn owned_image(id: i64, name: &str) -> bool {
    (0..super::super::MAX_EMBED_IMAGES).any(|slot| name == format!("img_{id}_{slot}.jpg"))
}

pub(super) fn image_id(name: &OsStr) -> Option<i64> {
    let name = name.to_str()?;
    let (id, _) = name
        .strip_prefix("img_")?
        .strip_suffix(".jpg")?
        .split_once('_')?;
    let id = id.parse::<i64>().ok()?;
    owned_image(id, name).then_some(id)
}
