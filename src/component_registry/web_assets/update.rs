use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{WebAssetComponent, WebAssetDelivery, WebAssetFile};

type CachedDeliveries = Option<(u64, &'static [WebAssetDelivery])>;
static CACHE: LazyLock<Mutex<CachedDeliveries>> = LazyLock::new(|| Mutex::new(None));

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

pub(super) fn deliveries() -> &'static [WebAssetDelivery] {
    let Some((sequence, value)) = super::super::update_catalog::contract("windows-web-assets-v1")
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

fn parse(value: serde_json::Value) -> Result<Vec<WebAssetDelivery>> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.windows.architecture != super::ARCHITECTURE
        || contract.windows.components.len() != 3
    {
        bail!("signed web-asset contract header is invalid");
    }
    super::super::catalog::validate_identifier(&contract.version)?;
    let version = leak(contract.version);
    let mut seen = HashSet::new();
    let mut deliveries = Vec::with_capacity(3);
    for delivery in contract.windows.components {
        let component = component(&delivery.id)?;
        if !seen.insert(component.id())
            || delivery.files.is_empty()
            || delivery.files.len() > super::MAX_ARCHIVE_ENTRIES
            || delivery.size_bytes == 0
            || delivery.unpacked_size_bytes == 0
        {
            bail!("signed web-asset delivery is invalid");
        }
        validate_sha(&delivery.sha256)?;
        super::super::update_catalog::validate_versioned_runtime_bundle_asset(
            &delivery.id,
            &delivery.asset,
            &delivery.download_url,
            &delivery.sha256,
            "zip",
        )?;
        let files = delivery
            .files
            .into_iter()
            .map(|file| {
                super::validate_relative_path(std::path::Path::new(&file.path))?;
                validate_sha(&file.sha256)?;
                if file.size_bytes == 0 {
                    bail!("signed web-asset file is empty");
                }
                Ok(WebAssetFile {
                    path: leak(file.path),
                    size_bytes: file.size_bytes,
                    sha256: leak(file.sha256),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        deliveries.push(WebAssetDelivery {
            component,
            version,
            asset: leak(delivery.asset),
            download_url: leak(delivery.download_url),
            size_bytes: delivery.size_bytes,
            sha256: leak(delivery.sha256),
            unpacked_size_bytes: delivery.unpacked_size_bytes,
            files: Box::leak(files.into_boxed_slice()),
        });
    }
    Ok(deliveries)
}

fn component(id: &str) -> Result<WebAssetComponent> {
    match id {
        "creation-3d-web" => Ok(WebAssetComponent::Creation3d),
        "prompt-dj-web" => Ok(WebAssetComponent::PromptDj),
        "tts-playground-web" => Ok(WebAssetComponent::TtsPlayground),
        _ => bail!("signed web-asset component is unknown"),
    }
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed web-asset digest is invalid");
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
            "../../../component-delivery/windows/web-assets-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
