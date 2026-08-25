use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{RecorderDelivery, RecorderFile};

type CachedDeliveries = Option<(u64, &'static [RecorderDelivery])>;
static CACHE: LazyLock<Mutex<CachedDeliveries>> = LazyLock::new(|| Mutex::new(None));
const LEGACY_WORKER_FILES: &[&str] = &[
    "bin/x64/sgt-recorder-worker.exe",
    "licenses/THIRD-PARTY-LICENSES.json",
    "licenses/THIRD-PARTY-NOTICES.txt",
];
const BUNDLE_REQUIRED_FILES: &[&str] = &[
    "bin/x64/sgt-recorder-worker.exe",
    "licenses/worker/THIRD-PARTY-LICENSES.json",
    "licenses/worker/THIRD-PARTY-NOTICES.txt",
    "web/index.html",
    "web/assets/index.js",
    "web/assets/index.css",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    schema_version: u32,
    architecture: String,
    components: Vec<Delivery>,
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

pub(super) fn deliveries() -> &'static [RecorderDelivery] {
    let Some((sequence, value)) = super::super::update_catalog::contract("windows-recorder-v1")
    else {
        return &[];
    };
    let mut cache = CACHE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some((cached_sequence, deliveries)) = *cache
        && cached_sequence == sequence
    {
        return deliveries;
    }
    let Ok(deliveries) = parse(value) else {
        return &[];
    };
    let deliveries = Box::leak(deliveries.into_boxed_slice());
    *cache = Some((sequence, deliveries));
    deliveries
}

fn parse(value: serde_json::Value) -> Result<Vec<RecorderDelivery>> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.architecture != super::ARCHITECTURE
        || !matches!(contract.components.len(), 1 | 2)
    {
        bail!("signed recorder contract header is invalid");
    }
    let mut seen = HashSet::new();
    let mut deliveries = Vec::with_capacity(contract.components.len());
    for delivery in contract.components {
        if !matches!(
            delivery.id.as_str(),
            super::BUNDLE_ID | super::WEB_ID | super::WORKER_ID
        ) || !seen.insert(delivery.id.clone())
            || delivery.files.is_empty()
            || delivery.files.len() > super::MAX_COMPONENT_FILES
            || delivery.size_bytes == 0
            || delivery.unpacked_size_bytes == 0
        {
            bail!("signed recorder delivery is invalid");
        }
        super::super::validate_identifier(&delivery.version)?;
        validate_sha(&delivery.sha256)?;
        super::super::update_catalog::validate_versioned_runtime_bundle_asset(
            &delivery.id,
            &delivery.asset,
            &delivery.download_url,
            &delivery.sha256,
            "zip",
        )?;
        let mut seen_files = HashSet::new();
        let files = delivery
            .files
            .into_iter()
            .map(|file| {
                super::super::receipt::validate_relative_path(std::path::Path::new(&file.path))?;
                if !seen_files.insert(file.path.clone()) {
                    bail!("signed recorder delivery repeats a file");
                }
                validate_sha(&file.sha256)?;
                if file.size_bytes == 0 {
                    bail!("signed recorder file is empty");
                }
                Ok(RecorderFile {
                    path: leak(file.path),
                    size_bytes: file.size_bytes,
                    sha256: leak(file.sha256),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        match delivery.id.as_str() {
            super::BUNDLE_ID => {
                if !BUNDLE_REQUIRED_FILES
                    .iter()
                    .all(|required| seen_files.contains(*required))
                    || seen_files.iter().any(|path| {
                        path.starts_with("bin/") && !BUNDLE_REQUIRED_FILES.contains(&path.as_str())
                    })
                {
                    bail!("signed recorder bundle inventory is incomplete");
                }
            }
            super::WORKER_ID
                if seen_files.len() != LEGACY_WORKER_FILES.len()
                    || !LEGACY_WORKER_FILES
                        .iter()
                        .all(|required| seen_files.contains(*required)) =>
            {
                bail!("signed recorder worker inventory is invalid");
            }
            super::WEB_ID
                if !["index.html", "assets/index.js", "assets/index.css"]
                    .iter()
                    .all(|required| seen_files.contains(*required)) =>
            {
                bail!("signed recorder web inventory is incomplete");
            }
            _ => {}
        }
        deliveries.push(RecorderDelivery {
            id: leak(delivery.id),
            version: leak(delivery.version),
            asset: leak(delivery.asset),
            download_url: leak(delivery.download_url),
            size_bytes: delivery.size_bytes,
            sha256: leak(delivery.sha256),
            unpacked_size_bytes: delivery.unpacked_size_bytes,
            files: Box::leak(files.into_boxed_slice()),
        });
    }
    let bundled = seen.len() == 1 && seen.contains(super::BUNDLE_ID);
    let legacy_pair =
        seen.len() == 2 && seen.contains(super::WEB_ID) && seen.contains(super::WORKER_ID);
    if !bundled && !legacy_pair {
        bail!("signed recorder contract mixes incompatible package layouts");
    }
    Ok(deliveries)
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed recorder digest is invalid");
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
            "../../../component-delivery/windows/recorder-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn consolidated_contract_parses_only_with_complete_inventory() {
    let files = BUNDLE_REQUIRED_FILES
        .iter()
        .map(|path| {
            serde_json::json!({
                "path": path,
                "sizeBytes": 1,
                "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
        })
        .collect::<Vec<_>>();
    let mut contract = serde_json::json!({
        "schemaVersion": 1,
        "architecture": "x64",
        "components": [{
            "id": "screen-recorder",
            "version": "5.5.0",
            "asset": "screen-recorder-5.5.0-aaaaaaaaaaaaaaaa.zip",
            "downloadUrl": "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/screen-recorder-5.5.0-aaaaaaaaaaaaaaaa.zip",
            "sizeBytes": 1,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "unpackedSizeBytes": 6,
            "files": files
        }]
    });
    assert!(parse(contract.clone()).is_ok());
    contract["components"][0]["files"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(parse(contract).is_err());
}
