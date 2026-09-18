use std::cmp::Reverse;
use std::io::Read;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::parse_verified;

// Stable identity of the append-only sgt-runtime-bundles release. Enumerating
// assets directly avoids downloading the growing embedded asset list in metadata.
const ASSETS_API: &str =
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/322595086/assets";
const PAGE_SIZE: usize = 100;
const PREFIX: &str = "sgt-component-catalog-v";
const MAX_RELEASE_RESPONSE: u64 = 2 * 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;

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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let assets = catalog_assets(|page| {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            bail!("component catalog enumeration timed out");
        }
        let response = crate::api::client::with_request_timeout(
            crate::api::client::UREQ_AGENT
                .get(format!("{ASSETS_API}?per_page={PAGE_SIZE}&page={page}"))
                .header("User-Agent", "ScreenGoatedToolbox-ComponentCatalog"),
            Some(remaining.min(std::time::Duration::from_secs(10))),
        )
        .call()
        .context("component catalog release lookup failed")?;
        let bytes = read_bounded(response.into_body().into_reader(), MAX_RELEASE_RESPONSE)?;
        serde_json::from_slice(&bytes).context("component catalog asset page is invalid")
    })?;
    let mut catalogs = assets
        .iter()
        .filter_map(|asset| parse_catalog_name(&asset.name).map(|parts| (parts, asset.clone())))
        .filter(|((sequence, _), _)| *sequence >= minimum_sequence)
        .collect::<Vec<_>>();
    catalogs.sort_by_key(|candidate| Reverse(candidate.0.0));
    for ((sequence, digest_prefix), catalog_asset) in catalogs {
        let stem = catalog_asset.name.trim_end_matches(".json");
        let signature_name = format!("{stem}.sig");
        let Some(signature_asset) = assets
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

fn catalog_assets(mut fetch: impl FnMut(u32) -> Result<Vec<Asset>>) -> Result<Vec<Asset>> {
    let mut catalogs = Vec::new();
    for page in 1.. {
        let assets = fetch(page)?;
        let count = assets.len();
        if count > PAGE_SIZE {
            bail!("component asset page exceeded its entry limit");
        }
        catalogs.extend(assets.into_iter().filter(|asset| {
            let json_name = asset
                .name
                .strip_suffix(".sig")
                .map(|stem| format!("{stem}.json"));
            parse_catalog_name(json_name.as_deref().unwrap_or(&asset.name)).is_some()
        }));
        if count < PAGE_SIZE {
            return Ok(catalogs);
        }
    }
    unreachable!("bounded enumeration deadline stops before page-number exhaustion")
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
    fn catalog_and_signature_can_follow_many_unrelated_pages() {
        let mut pages = 0;
        let assets = super::catalog_assets(|page| {
            pages += 1;
            let count = if page <= 4 { super::PAGE_SIZE } else { 1 };
            Ok((0..count)
                .map(|index| super::Asset {
                    name: if page == 4 && index == 99 {
                        "sgt-component-catalog-v000012-0123456789abcdef.json".into()
                    } else if page == 5 {
                        "sgt-component-catalog-v000012-0123456789abcdef.sig".into()
                    } else {
                        format!("package-{page}-{index}.zip")
                    },
                    size: 64,
                    browser_download_url: String::new(),
                })
                .collect())
        })
        .unwrap();
        assert_eq!(pages, 5);
        assert_eq!(assets.len(), 2);
        assert!(assets[0].name.ends_with(".json"));
        assert!(assets[1].name.ends_with(".sig"));
    }

    #[test]
    fn failed_page_does_not_return_a_partial_catalog_listing() {
        assert!(super::catalog_assets(|_| anyhow::bail!("network failed")).is_err());
        assert!(super::read_bounded(std::io::Cursor::new(b"1234"), 3).is_err());
    }

    #[test]
    #[ignore = "fetches and verifies the published component catalog without activating it"]
    fn published_catalog_verifies_after_paginated_discovery() {
        let candidate = super::fetch_highest_compatible(0).unwrap();
        super::super::parse_verified(&candidate.catalog, &candidate.signature).unwrap();
        println!(
            "verified component catalog {} bytes={}",
            candidate.name,
            candidate.catalog.len()
        );
    }

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
