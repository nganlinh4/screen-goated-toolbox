use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{CreationDelivery, CreationFile};

static CACHE: LazyLock<Mutex<Option<(u64, &'static CreationDelivery)>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Contract {
    schema_version: u32,
    host_version: String,
    version: String,
    runtime_version: String,
    features: Vec<String>,
    windows: Platform,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Platform {
    architecture: String,
    asset: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
    unpacked_size_bytes: u64,
    files: Vec<File>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    path: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn delivery() -> Option<&'static CreationDelivery> {
    let (sequence, value) = super::super::update_catalog::contract("windows-creation-v1")?;
    let mut cache = CACHE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some((cached, delivery)) = *cache
        && cached == sequence
    {
        return Some(delivery);
    }
    let parsed = parse(value).ok()?;
    let parsed = Box::leak(Box::new(parsed));
    *cache = Some((sequence, parsed));
    Some(parsed)
}

fn parse(value: serde_json::Value) -> Result<CreationDelivery> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.host_version != env!("CARGO_PKG_VERSION")
        || contract.windows.architecture != super::ARCHITECTURE
        || contract.windows.size_bytes == 0
        || contract.windows.unpacked_size_bytes == 0
        || contract.windows.files.len() != 4
    {
        bail!("signed Creation contract header is invalid");
    }
    super::super::validate_identifier(&contract.version)?;
    super::super::validate_identifier(&contract.runtime_version)?;
    let features = contract.features.into_iter().collect::<HashSet<_>>();
    if features
        != HashSet::from([
            "image_to_3d".into(),
            "image_to_svg".into(),
            "image_creator".into(),
        ])
    {
        bail!("signed Creation capabilities are invalid")
    }
    validate_sha(&contract.windows.sha256)?;
    let expected_asset = format!(
        "creation-windows-{}-{}.zip",
        contract.version,
        &contract.windows.sha256[..16]
    );
    if contract.windows.asset != expected_asset {
        bail!("signed Creation archive is not content-addressed")
    }
    super::super::update_catalog::validate_versioned_runtime_bundle_asset(
        "creation-windows",
        &contract.windows.asset,
        &contract.windows.download_url,
        &contract.windows.sha256,
        "zip",
    )?;
    let mut seen = HashSet::new();
    let mut unpacked = 0_u64;
    let files = contract
        .windows
        .files
        .into_iter()
        .map(|file| {
            crate::component_registry::receipt::validate_relative_path(std::path::Path::new(
                &file.path,
            ))?;
            validate_sha(&file.sha256)?;
            if file.size_bytes == 0 || !seen.insert(file.path.clone()) {
                bail!("signed Creation file is invalid")
            }
            unpacked = unpacked
                .checked_add(file.size_bytes)
                .ok_or_else(|| anyhow::anyhow!("signed Creation inventory is too large"))?;
            Ok(CreationFile {
                path: leak(file.path),
                size_bytes: file.size_bytes,
                sha256: leak(file.sha256),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = HashSet::from([
        "bin/sgt_creation_runtime.exe",
        "web/assets/index.css",
        "web/assets/index.js",
        "web/index.html",
    ]);
    if seen.iter().map(String::as_str).collect::<HashSet<_>>() != expected
        || unpacked != contract.windows.unpacked_size_bytes
    {
        bail!("signed Creation inventory is incomplete")
    }
    let features = features.into_iter().map(leak).collect::<Vec<_>>();
    Ok(CreationDelivery {
        version: leak(contract.version),
        runtime_version: leak(contract.runtime_version),
        features: Box::leak(features.into_boxed_slice()),
        asset: leak(contract.windows.asset),
        download_url: leak(contract.windows.download_url),
        size_bytes: contract.windows.size_bytes,
        sha256: leak(contract.windows.sha256),
        unpacked_size_bytes: contract.windows.unpacked_size_bytes,
        files: Box::leak(files.into_boxed_slice()),
    })
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed Creation digest is invalid")
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
            "../../../component-delivery/windows/creation-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
