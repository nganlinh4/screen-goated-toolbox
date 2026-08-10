use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{ExternalArchiveFormat, ExternalToolDelivery, ExternalToolFile};

type CachedDeliveries = Option<(u64, &'static [ExternalToolDelivery])>;
static CACHE: LazyLock<Mutex<CachedDeliveries>> = LazyLock::new(|| Mutex::new(None));

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
    archive_format: String,
    size_bytes: u64,
    sha256: String,
    unpacked_size_bytes: u64,
    files: Vec<File>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    path: String,
    archive_path: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn deliveries() -> &'static [ExternalToolDelivery] {
    let Some((sequence, value)) =
        super::super::update_catalog::contract("windows-external-tools-v1")
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

fn parse(value: serde_json::Value) -> Result<Vec<ExternalToolDelivery>> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.architecture != super::ARCHITECTURE
        || contract.components.len() != super::ExternalTool::ALL.len()
    {
        bail!("signed external-tool contract header is invalid");
    }
    let mut deliveries = Vec::with_capacity(contract.components.len());
    for delivery in contract.components {
        if !super::ExternalTool::ALL
            .iter()
            .any(|tool| tool.id() == delivery.id)
            || delivery.files.is_empty()
            || delivery.files.len() > super::MAX_COMPONENT_FILES
            || delivery.size_bytes == 0
            || delivery.unpacked_size_bytes == 0
        {
            bail!("signed external-tool delivery is invalid");
        }
        super::super::catalog::validate_identifier(&delivery.version)?;
        validate_sha(&delivery.sha256)?;
        validate_url(&delivery.download_url)?;
        let archive_format = match delivery.archive_format.as_str() {
            "raw" => ExternalArchiveFormat::Raw,
            "zip" => ExternalArchiveFormat::Zip,
            _ => bail!("signed external-tool archive format is unsupported"),
        };
        let files = delivery
            .files
            .into_iter()
            .map(|file| {
                super::super::receipt::validate_relative_path(std::path::Path::new(&file.path))?;
                super::super::receipt::validate_relative_path(std::path::Path::new(
                    &file.archive_path,
                ))?;
                validate_sha(&file.sha256)?;
                if file.size_bytes == 0 {
                    bail!("signed external-tool file is empty");
                }
                Ok(ExternalToolFile {
                    path: leak(file.path),
                    archive_path: leak(file.archive_path),
                    size_bytes: file.size_bytes,
                    sha256: leak(file.sha256),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        deliveries.push(ExternalToolDelivery {
            id: leak(delivery.id),
            version: leak(delivery.version),
            asset: leak(delivery.asset),
            download_url: leak(delivery.download_url),
            archive_format,
            size_bytes: delivery.size_bytes,
            sha256: leak(delivery.sha256),
            unpacked_size_bytes: delivery.unpacked_size_bytes,
            files: Box::leak(files.into_boxed_slice()),
        });
    }
    Ok(deliveries)
}

fn validate_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value)?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        bail!("signed external-tool URL is invalid");
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed external-tool digest is invalid");
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
            "../../../component-delivery/windows/external-tools-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
