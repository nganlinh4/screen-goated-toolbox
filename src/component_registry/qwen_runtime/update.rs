use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{QwenRuntimeArchive, QwenRuntimeDelivery, QwenRuntimeFile};

static CACHE: LazyLock<Mutex<Option<(u64, &'static QwenRuntimeDelivery)>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    schema_version: u32,
    version: String,
    windows: Platform,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Platform {
    architecture: String,
    components: Vec<Delivery>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    id: String,
    dependencies: Vec<String>,
    assets: Vec<Archive>,
    unpacked_size_bytes: u64,
    files: Vec<File>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Archive {
    download_url: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    archive_index: usize,
    archive_path: String,
    path: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn delivery() -> Option<&'static QwenRuntimeDelivery> {
    let (sequence, value) = super::super::update_catalog::contract("windows-qwen-runtime-v1")?;
    let mut cache = CACHE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some((cached_sequence, delivery)) = *cache
        && cached_sequence == sequence
    {
        return Some(delivery);
    }
    let Ok(delivery) = parse(value) else {
        return None;
    };
    let delivery = Box::leak(Box::new(delivery));
    *cache = Some((sequence, delivery));
    Some(delivery)
}

fn parse(value: serde_json::Value) -> Result<QwenRuntimeDelivery> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.windows.architecture != super::ARCHITECTURE
        || contract.windows.components.len() != 1
    {
        bail!("signed Qwen runtime contract header is invalid");
    }
    super::super::validate_identifier(&contract.version)?;
    let delivery = contract.windows.components.into_iter().next().unwrap();
    if delivery.id != super::COMPONENT_ID
        || delivery.dependencies != [super::VC_COMPONENT_ID]
        || delivery.assets.is_empty()
        || delivery.assets.len() > 8
        || delivery.files.is_empty()
        || delivery.files.len() > super::MAX_COMPONENT_FILES
        || delivery.unpacked_size_bytes == 0
    {
        bail!("signed Qwen runtime delivery is invalid");
    }
    let archives = delivery
        .assets
        .into_iter()
        .map(|archive| {
            validate_sha(&archive.sha256)?;
            validate_url(&archive.download_url)?;
            if archive.size_bytes == 0 {
                bail!("signed Qwen runtime archive is empty");
            }
            Ok(QwenRuntimeArchive {
                url: leak(archive.download_url),
                size_bytes: archive.size_bytes,
                sha256: leak(archive.sha256),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let archive_count = archives.len();
    let files = delivery
        .files
        .into_iter()
        .map(|file| {
            if file.archive_index >= archive_count || file.size_bytes == 0 {
                bail!("signed Qwen runtime file is invalid");
            }
            super::super::receipt::validate_relative_path(std::path::Path::new(&file.path))?;
            super::super::receipt::validate_relative_path(std::path::Path::new(
                &file.archive_path,
            ))?;
            validate_sha(&file.sha256)?;
            Ok(QwenRuntimeFile {
                archive_index: file.archive_index,
                archive_path: leak(file.archive_path),
                path: leak(file.path),
                size_bytes: file.size_bytes,
                sha256: leak(file.sha256),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(QwenRuntimeDelivery {
        version: leak(contract.version),
        archives: Box::leak(archives.into_boxed_slice()),
        unpacked_size_bytes: delivery.unpacked_size_bytes,
        files: Box::leak(files.into_boxed_slice()),
    })
}

fn validate_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value)?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        bail!("signed Qwen runtime URL is invalid");
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed Qwen runtime digest is invalid");
    }
    Ok(())
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

#[test]
fn tracked_contract_parses() {
    parse(
        serde_json::from_str(include_str!(
            "../../../component-delivery/windows/qwen-runtime-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
