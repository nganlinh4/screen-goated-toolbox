use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{DetectorDelivery, DetectorFile};

static CACHE: LazyLock<Mutex<Option<(u64, &'static DetectorDelivery)>>> =
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

pub(super) fn delivery() -> Option<&'static DetectorDelivery> {
    let (sequence, value) =
        super::super::update_catalog::contract("windows-screen-text-detector-v1")?;
    let mut cache = CACHE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some((cached_sequence, delivery)) = *cache
        && cached_sequence == sequence
    {
        return Some(delivery);
    }
    let delivery = Box::leak(Box::new(parse(value).ok()?));
    *cache = Some((sequence, delivery));
    Some(delivery)
}

fn parse(value: serde_json::Value) -> Result<DetectorDelivery> {
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
        bail!("signed Screen Translate detector delivery is invalid");
    }
    super::super::validate_identifier(&delivery.version)?;
    validate_sha(&delivery.sha256)?;
    let expected_asset = format!(
        "{}-{}-{}.zip",
        delivery.id,
        delivery.version,
        &delivery.sha256[..16]
    );
    if delivery.asset != expected_asset {
        bail!("signed Screen Translate detector asset is not content-addressed");
    }
    super::super::update_catalog::validate_runtime_bundle_asset(
        &delivery.asset,
        &delivery.download_url,
        &delivery.sha256,
        "zip",
    )?;
    let files = delivery
        .files
        .into_iter()
        .map(|file| {
            super::super::receipt::validate_relative_path(std::path::Path::new(&file.path))?;
            validate_sha(&file.sha256)?;
            if file.size_bytes == 0 {
                bail!("signed Screen Translate detector file is empty");
            }
            Ok(DetectorFile {
                path: leak(file.path),
                size_bytes: file.size_bytes,
                sha256: leak(file.sha256),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(DetectorDelivery {
        version: leak(delivery.version),
        asset: leak(delivery.asset),
        download_url: leak(delivery.download_url),
        size_bytes: delivery.size_bytes,
        sha256: leak(delivery.sha256),
        unpacked_size_bytes: delivery.unpacked_size_bytes,
        files: Box::leak(files.into_boxed_slice()),
    })
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed Screen Translate detector digest is invalid");
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
            "../../../component-delivery/windows/screen-text-detector-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
