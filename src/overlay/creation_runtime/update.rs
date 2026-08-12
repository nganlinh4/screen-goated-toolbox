use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::RuntimeDelivery;

static CACHE: LazyLock<Mutex<Option<(u64, &'static RuntimeDelivery)>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    schema_version: u32,
    host_version: String,
    version: String,
    features: Vec<String>,
    windows: Delivery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    asset: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn delivery() -> Option<&'static RuntimeDelivery> {
    let (sequence, value) =
        crate::component_registry::update_catalog::contract("creation-runtime-v1")?;
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

fn parse(value: serde_json::Value) -> Result<RuntimeDelivery> {
    let contract: Contract = serde_json::from_value(value)?;
    if contract.schema_version != 1
        || contract.host_version != env!("CARGO_PKG_VERSION")
        || contract.features.is_empty()
        || contract.features.len() > 16
        || contract.windows.size_bytes == 0
    {
        bail!("signed creation-runtime contract is invalid");
    }
    crate::component_registry::validate_identifier(&contract.version)?;
    validate_sha(&contract.windows.sha256)?;
    let expected_asset = format!(
        "sgt-creation-runtime-windows-x64-{}.exe",
        &contract.windows.sha256[..16]
    );
    if contract.windows.asset != expected_asset {
        bail!("signed creation-runtime asset is not content-addressed");
    }
    crate::component_registry::update_catalog::validate_runtime_bundle_asset(
        &contract.windows.asset,
        &contract.windows.download_url,
        &contract.windows.sha256,
        "exe",
    )?;
    let features = contract
        .features
        .into_iter()
        .map(|feature| {
            crate::component_registry::validate_identifier(&feature)?;
            Ok(leak(feature))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RuntimeDelivery {
        version: leak(contract.version),
        features: Box::leak(features.into_boxed_slice()),
        asset: leak(contract.windows.asset),
        download_url: leak(contract.windows.download_url),
        size_bytes: contract.windows.size_bytes,
        sha256: leak(contract.windows.sha256),
    })
}

fn validate_sha(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("signed creation-runtime digest is invalid");
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
            "../../../component-delivery/creation-runtime-v1.json"
        ))
        .unwrap(),
    )
    .unwrap();
}
