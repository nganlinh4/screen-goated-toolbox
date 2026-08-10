use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{EngineDelivery, EngineFile};

static CACHE: LazyLock<Mutex<Option<(u64, &'static EngineDelivery)>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    schema_version: u32,
    architecture: String,
    component: Delivery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    id: String,
    version: String,
    asset: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
    unpacked_size_bytes: u64,
    files: Vec<File>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    path: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn delivery() -> Option<&'static EngineDelivery> {
    let (sequence, value) = super::super::update_catalog::contract("windows-computer-control-v1")?;
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

fn parse(value: serde_json::Value) -> Result<EngineDelivery> {
    let contract: Contract = serde_json::from_value(value)?;
    let delivery = contract.component;
    if contract.schema_version != 1
        || contract.architecture != super::ARCHITECTURE
        || delivery.id != super::ID
        || delivery.files.is_empty()
        || delivery.files.len() > super::MAX_COMPONENT_FILES
        || delivery.size_bytes == 0
        || delivery.unpacked_size_bytes == 0
    {
        bail!("signed computer-control delivery is invalid");
    }
    super::super::validate_identifier(&delivery.version)?;
    validate_sha(&delivery.sha256)?;
    validate_url(&delivery.download_url)?;
    let files = delivery
        .files
        .into_iter()
        .map(|file| {
            super::super::receipt::validate_relative_path(std::path::Path::new(&file.path))?;
            validate_sha(&file.sha256)?;
            if file.size_bytes == 0 {
                bail!("signed computer-control file is empty");
            }
            Ok(EngineFile {
                path: leak(file.path),
                size_bytes: file.size_bytes,
                sha256: leak(file.sha256),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EngineDelivery {
        version: leak(delivery.version),
        asset: leak(delivery.asset),
        download_url: leak(delivery.download_url),
        size_bytes: delivery.size_bytes,
        sha256: leak(delivery.sha256),
        unpacked_size_bytes: delivery.unpacked_size_bytes,
        files: Box::leak(files.into_boxed_slice()),
    })
}

fn validate_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value)?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        bail!("signed computer-control URL is invalid");
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed computer-control digest is invalid");
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
            "../../../component-delivery/windows/computer-control-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
