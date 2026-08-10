use std::io::Read;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::parse_verified;

const RELEASE_API: &str = "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/tags/sgt-runtime-bundles";
const PREFIX: &str = "sgt-component-catalog-v";
const MAX_RELEASE_RESPONSE: u64 = 2 * 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}

pub(super) struct Candidate {
    pub(super) name: String,
    pub(super) catalog: Vec<u8>,
    pub(super) signature: Vec<u8>,
}

pub(super) fn fetch_highest_compatible(minimum_sequence: u64) -> Result<Candidate> {
    let response = crate::api::client::with_request_timeout(
        crate::api::client::UREQ_AGENT
            .get(RELEASE_API)
            .header("User-Agent", "ScreenGoatedToolbox-ComponentCatalog"),
        Some(std::time::Duration::from_secs(10)),
    )
    .call()
    .context("component catalog release lookup failed")?;
    let bytes = read_bounded(response.into_body().into_reader(), MAX_RELEASE_RESPONSE)?;
    let release: Release =
        serde_json::from_slice(&bytes).context("component catalog release response is invalid")?;
    if release.assets.len() > 256 {
        bail!("component catalog release has too many assets");
    }
    let mut catalogs = release
        .assets
        .iter()
        .filter_map(|asset| parse_catalog_name(&asset.name).map(|parts| (parts, asset.clone())))
        .filter(|((sequence, _), _)| *sequence >= minimum_sequence)
        .collect::<Vec<_>>();
    catalogs.sort_by(|left, right| right.0.0.cmp(&left.0.0));
    for ((sequence, digest_prefix), catalog_asset) in catalogs {
        let stem = catalog_asset.name.trim_end_matches(".json");
        let signature_name = format!("{stem}.sig");
        let Some(signature_asset) = release
            .assets
            .iter()
            .find(|asset| asset.name == signature_name && asset.size == 64)
        else {
            continue;
        };
        let catalog = download(&catalog_asset, MAX_CATALOG_BYTES)?;
        let actual = format!("{:x}", Sha256::digest(&catalog));
        if !actual.starts_with(&digest_prefix) {
            continue;
        }
        let signature = download(signature_asset, 64)?;
        let Ok(parsed) = parse_verified(&catalog, &signature) else {
            continue;
        };
        if parsed.sequence != sequence {
            continue;
        }
        return Ok(Candidate {
            name: catalog_asset.name,
            catalog,
            signature,
        });
    }
    bail!("no newer compatible signed component catalog is available")
}

fn parse_catalog_name(name: &str) -> Option<(u64, String)> {
    let body = name.strip_prefix(PREFIX)?.strip_suffix(".json")?;
    let (sequence, digest) = body.split_once('-')?;
    if sequence.len() != 6
        || digest.len() != 16
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some((sequence.parse().ok()?, digest.to_ascii_lowercase()))
}

fn download(asset: &Asset, maximum: u64) -> Result<Vec<u8>> {
    if asset.size == 0 || asset.size > maximum {
        bail!("component catalog asset has an invalid declared size");
    }
    let response = crate::api::client::with_request_timeout(
        crate::api::client::UREQ_DOWNLOAD_AGENT
            .get(&asset.browser_download_url)
            .header("User-Agent", "ScreenGoatedToolbox-ComponentCatalog"),
        Some(std::time::Duration::from_secs(20)),
    )
    .call()
    .with_context(|| format!("component catalog asset download failed: {}", asset.name))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != asset.size)
    {
        bail!("component catalog response length is invalid");
    }
    let bytes = read_bounded(response.into_body().into_reader(), asset.size)?;
    if bytes.len() as u64 != asset.size {
        bail!("component catalog asset length is invalid");
    }
    Ok(bytes)
}

fn read_bounded(mut reader: impl Read, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("component catalog response exceeded its size limit");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::parse_catalog_name;

    #[test]
    fn catalog_asset_identity_is_strict() {
        assert_eq!(
            parse_catalog_name("sgt-component-catalog-v000012-0123456789abcdef.json"),
            Some((12, "0123456789abcdef".to_string()))
        );
        for invalid in [
            "sgt-component-catalog-v12-0123456789abcdef.json",
            "sgt-component-catalog-v000012-0123456789abcdeg.json",
            "sgt-component-catalog-v000012-0123456789abcdef.sig",
            "prefix-sgt-component-catalog-v000012-0123456789abcdef.json",
        ] {
            assert_eq!(parse_catalog_name(invalid), None);
        }
    }
}
