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
    protocol: Option<String>,
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
    super::protocol::validate(&delivery.version, delivery.protocol.as_deref())
        .map_err(anyhow::Error::msg)?;
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
    if ![super::EXECUTABLE_PATH, super::READER_CATALOG_PATH]
        .iter()
        .all(|required| delivery.files.iter().any(|file| file.path == *required))
    {
        bail!("signed Screen Translate detector inventory does not match its protocol");
    }
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

#[cfg(test)]
fn new_contract() -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../component-delivery/windows/screen-text-detector-v1.json"
    ))
    .unwrap();
    let component = value["component"].as_object_mut().unwrap();
    component.insert("version".into(), "5.0.0".into());
    let asset = format!(
        "screen-text-detector-5.0.0-{}.zip",
        &component["sha256"].as_str().unwrap()[..16]
    );
    component.insert("asset".into(), asset.clone().into());
    component.insert(
        "downloadUrl".into(),
        format!(
            "{}{asset}",
            super::super::update_catalog::RUNTIME_BUNDLES_PREFIX
        )
        .into(),
    );
    value
}

#[test]
fn an_update_without_compatibility_metadata_cannot_switch_to_legacy() {
    assert!(parse(new_contract()).is_err());
}

#[test]
fn a_compatible_update_keeps_its_declared_protocol_and_own_identity() {
    let mut value = new_contract();
    value["component"]["protocol"] = "sgts-v1".into();
    let delivery = parse(value).unwrap();
    assert_eq!(delivery.version, "5.0.0");
}

#[test]
fn unknown_protocols_are_rejected_before_delivery_activation() {
    let mut value = new_contract();
    value["component"]["protocol"] = "sgts-v2".into();
    assert!(parse(value).is_err());
}

/// Adds the legacy recognizer catalog, so only the protocol can refuse it.
#[cfg(test)]
fn with_legacy_inventory(mut value: serde_json::Value) -> serde_json::Value {
    let files = value["component"]["files"].as_array_mut().unwrap();
    let mut catalog = files[0].clone();
    catalog["path"] = "models/pp-ocr-screen-text/recognizers.json".into();
    files.push(catalog);
    value
}

#[test]
fn a_signed_update_cannot_select_the_retired_legacy_detector() {
    let mut value = with_legacy_inventory(new_contract());
    value["component"]["protocol"] = "sgtd-v4".into();
    let error = parse(value).err().expect("legacy protocol must be refused");
    assert!(error.to_string().contains("retired"), "{error:#}");
}

#[test]
fn the_last_legacy_release_cannot_return_through_metadata_migration() {
    let mut value = with_legacy_inventory(new_contract());
    let component = value["component"].as_object_mut().unwrap();
    component.insert("version".into(), "3.2.18".into());
    let asset = format!(
        "screen-text-detector-3.2.18-{}.zip",
        &component["sha256"].as_str().unwrap()[..16]
    );
    component.insert(
        "downloadUrl".into(),
        format!(
            "{}{asset}",
            super::super::update_catalog::RUNTIME_BUNDLES_PREFIX
        )
        .into(),
    );
    component.insert("asset".into(), asset.into());
    assert!(parse(value).is_err());
}
