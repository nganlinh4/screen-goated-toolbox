//! Signed, append-only discovery for independently updateable components.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, RwLock, TryLockError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::Deserialize;
use serde_json::Value;

mod cache;
mod network;
mod signature;

const MAX_CONTRACTS: usize = 64;
const MAX_POLICIES: usize = 128;
const PLATFORM: &str = "windows-x64";

static ACTIVE: LazyLock<RwLock<Option<Arc<UpdateCatalog>>>> = LazyLock::new(|| RwLock::new(None));
static REFRESH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static LAST_POLICY_CHECK: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) const RUNTIME_BUNDLES_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateCatalog {
    schema_version: u32,
    sequence: u64,
    channel: String,
    min_host_version: String,
    max_host_version_exclusive: String,
    contracts: Vec<CatalogContract>,
    policies: Vec<CatalogPolicy>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogContract {
    name: String,
    platform: String,
    delivery: Value,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogPolicy {
    id: String,
    mode: String,
    check_hours: u64,
    group: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedCatalogBaseline {
    schema_version: u32,
    sequence: u64,
}

pub(crate) fn refresh_in_background() {
    if cfg!(sgt_staging_delivery) {
        crate::log_info!("[Component updates] Production catalog disabled for staging debug build");
        return;
    }
    if let Ok(Some(catalog)) = cache::load_highest() {
        activate(catalog);
        super::external_tools::schedule_periodic_updates();
    }
    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Io,
        "component-catalog-refresh",
        || match refresh_now() {
            Ok(_) => super::external_tools::schedule_periodic_updates(),
            Err(error) => {
                crate::log_info!("[Component updates] Catalog refresh skipped: {error:#}")
            }
        },
    );
}

pub(crate) fn refresh_now() -> Result<u64> {
    if cfg!(sgt_staging_delivery) {
        bail!("production component updates are disabled for staging debug builds");
    }
    let _refresh = REFRESH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = refresh_now_locked();
    mark_all_policy_checks();
    result
}

fn refresh_now_locked() -> Result<u64> {
    let minimum = ACTIVE
        .read()
        .ok()
        .and_then(|catalog| catalog.as_ref().map(|catalog| catalog.sequence))
        .unwrap_or(0);
    let candidate = network::fetch_highest_compatible(minimum)?;
    cache::store(&candidate.name, &candidate.catalog, &candidate.signature)?;
    let catalog = parse_verified(&candidate.catalog, &candidate.signature)?;
    let sequence = catalog.sequence;
    activate(catalog);
    Ok(sequence)
}

pub(crate) fn refresh_due(id: &str, expected_mode: &str) -> bool {
    if cfg!(sgt_staging_delivery) {
        return false;
    }
    let Some((mode, check_hours, group)) = policy(id) else {
        return false;
    };
    mode == expected_mode && policy_group_due(&group, check_hours)
}

pub(crate) fn refresh_for_use(id: &str, expected_mode: &str) -> bool {
    if cfg!(sgt_staging_delivery) {
        return false;
    }
    let Some((mode, check_hours, group)) = policy(id) else {
        return false;
    };
    if mode != expected_mode || !policy_group_due(&group, check_hours) {
        return false;
    }
    let _refresh = match REFRESH_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => return false,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    if !policy_group_due(&group, check_hours) {
        return false;
    }
    LAST_POLICY_CHECK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(group, Instant::now());
    let before = active_sequence();
    match refresh_now_locked() {
        Ok(after) => after > before,
        Err(error) => {
            crate::log_info!("[Component updates] {id} refresh skipped: {error:#}");
            false
        }
    }
}

pub(crate) fn validate_runtime_bundle_asset(
    asset: &str,
    download_url: &str,
    sha256: &str,
    extension: &str,
) -> Result<()> {
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("runtime-bundles digest is invalid");
    }
    if asset.is_empty()
        || asset.contains(['/', '\\'])
        || !asset.ends_with(&format!("-{}.{}", &sha256[..16], extension))
        || download_url != format!("{RUNTIME_BUNDLES_PREFIX}{asset}")
    {
        bail!("runtime-bundles asset is not immutable and content-addressed");
    }
    Ok(())
}

pub(crate) fn validate_versioned_runtime_bundle_asset(
    component_id: &str,
    asset: &str,
    download_url: &str,
    sha256: &str,
    extension: &str,
) -> Result<()> {
    validate_runtime_bundle_asset(asset, download_url, sha256, extension)?;
    let prefix = format!("{component_id}-");
    let suffix = format!("-{}.{}", &sha256[..16], extension);
    let asset_version = asset
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(&suffix))
        .ok_or_else(|| {
            anyhow::anyhow!("runtime-bundles asset has an invalid component identity")
        })?;
    super::validate_identifier(asset_version)
}

pub(crate) fn contract(name: &str) -> Option<(u64, Value)> {
    if cfg!(sgt_staging_delivery) {
        return None;
    }
    ACTIVE
        .read()
        .ok()
        .and_then(|catalog| catalog.clone())
        .and_then(|catalog| contract_from(&catalog, name, embedded_catalog_sequence()))
}

fn contract_from(
    catalog: &UpdateCatalog,
    name: &str,
    minimum_sequence: u64,
) -> Option<(u64, Value)> {
    if catalog.sequence < minimum_sequence {
        return None;
    }
    catalog
        .contracts
        .iter()
        .find(|contract| {
            contract.name == name && matches!(contract.platform.as_str(), PLATFORM | "multi")
        })
        .map(|contract| (catalog.sequence, contract.delivery.clone()))
}

fn embedded_catalog_sequence() -> u64 {
    static SEQUENCE: LazyLock<u64> = LazyLock::new(|| {
        let baseline: EmbeddedCatalogBaseline = serde_json::from_str(include_str!(
            "../../component-delivery/update-catalog-v1.sources.json"
        ))
        .expect("tracked component catalog source must be valid JSON");
        assert_eq!(baseline.schema_version, 1);
        assert!(baseline.sequence > 0);
        baseline.sequence
    });
    *SEQUENCE
}

pub(crate) fn policy(id: &str) -> Option<(String, u64, String)> {
    if cfg!(sgt_staging_delivery) {
        return None;
    }
    ACTIVE
        .read()
        .ok()
        .and_then(|catalog| catalog.clone())
        .and_then(|catalog| {
            catalog
                .policies
                .iter()
                .find(|policy| policy.id == id)
                .map(|policy| {
                    (
                        policy.mode.clone(),
                        policy.check_hours,
                        policy.group.clone(),
                    )
                })
        })
}

fn activate(catalog: UpdateCatalog) {
    if let Ok(mut active) = ACTIVE.write()
        && active
            .as_ref()
            .is_none_or(|current| catalog.sequence > current.sequence)
    {
        *active = Some(Arc::new(catalog));
    }
}

fn active_sequence() -> u64 {
    ACTIVE
        .read()
        .ok()
        .and_then(|catalog| catalog.as_ref().map(|catalog| catalog.sequence))
        .unwrap_or(0)
}

fn policy_group_due(group: &str, check_hours: u64) -> bool {
    LAST_POLICY_CHECK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(group)
        .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(check_hours * 60 * 60))
}

fn mark_all_policy_checks() {
    let groups = ACTIVE
        .read()
        .ok()
        .and_then(|catalog| catalog.clone())
        .map(|catalog| {
            catalog
                .policies
                .iter()
                .map(|policy| policy.group.clone())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let now = Instant::now();
    let mut checks = LAST_POLICY_CHECK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for group in groups {
        checks.insert(group, now);
    }
}

fn parse_verified(catalog_bytes: &[u8], signature_bytes: &[u8]) -> Result<UpdateCatalog> {
    signature::verify(catalog_bytes, signature_bytes)?;
    let catalog: UpdateCatalog = serde_json::from_slice(catalog_bytes)
        .context("signed component catalog is not valid JSON")?;
    validate(&catalog)?;
    Ok(catalog)
}

fn validate(catalog: &UpdateCatalog) -> Result<()> {
    if catalog.schema_version != 1 || catalog.sequence == 0 || catalog.channel != "stable" {
        bail!("signed component catalog header is invalid");
    }
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let minimum = Version::parse(&catalog.min_host_version)?;
    let maximum = Version::parse(&catalog.max_host_version_exclusive)?;
    if current < minimum || current >= maximum {
        bail!("signed component catalog is not compatible with this host");
    }
    if catalog.contracts.is_empty() || catalog.contracts.len() > MAX_CONTRACTS {
        bail!("signed component catalog has an invalid contract count");
    }
    if catalog.policies.len() > MAX_POLICIES {
        bail!("signed component catalog has too many policies");
    }
    let mut names = HashSet::new();
    for contract in &catalog.contracts {
        validate_token(&contract.name, 96)?;
        validate_token(&contract.platform, 32)?;
        if !names.insert(&contract.name) || !contract.delivery.is_object() {
            bail!("signed component catalog contains an invalid contract");
        }
    }
    let mut ids = HashSet::new();
    for policy in &catalog.policies {
        super::catalog::validate_identifier(&policy.id)?;
        validate_token(&policy.mode, 32)?;
        validate_token(&policy.group, 64)?;
        if policy.check_hours == 0 || policy.check_hours > 24 * 365 || !ids.insert(&policy.id) {
            bail!("signed component catalog contains an invalid policy");
        }
    }
    Ok(())
}

fn validate_token(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > maximum
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("signed component catalog token is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
