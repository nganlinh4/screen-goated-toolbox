use super::UpdateCandidate;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const MANIFEST_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.json";
const SIGNATURE_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.sig";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/component-delivery/update-catalog-p256-public-key.hex"
));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StableManifest {
    schema_version: u32,
    channel: String,
    version: String,
    #[serde(default)]
    release_notes: String,
    installer: Installer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Installer {
    name: String,
    url: String,
    size_bytes: u64,
    sha256: String,
}

pub(super) fn fetch() -> Result<Option<UpdateCandidate>> {
    let Some(payload) = fetch_optional(MANIFEST_URL, MAX_MANIFEST_BYTES)? else {
        return Ok(None);
    };
    let signature = fetch_required(SIGNATURE_URL, 64)?;
    let public_key = crate::crypto::decode_hex(PUBLIC_KEY_HEX.trim(), "app update manifest")?;
    crate::crypto::verify_p256_sha256(&public_key, &payload, &signature, "app update manifest")?;
    parse(&payload).map(Some)
}

fn parse(payload: &[u8]) -> Result<UpdateCandidate> {
    let manifest: StableManifest =
        serde_json::from_slice(payload).context("app update manifest JSON is invalid")?;
    if manifest.schema_version != 1 || manifest.channel != "stable" {
        bail!("app update manifest contract is unsupported");
    }
    let candidate = UpdateCandidate {
        version: semver::Version::parse(&manifest.version)
            .context("app update manifest version is invalid")?,
        body: manifest.release_notes,
        asset_name: manifest.installer.name,
        download_url: manifest.installer.url,
        size_bytes: manifest.installer.size_bytes,
        sha256: manifest.installer.sha256,
    };
    candidate.validate()?;
    Ok(candidate)
}

fn fetch_optional(url: &str, limit: u64) -> Result<Option<Vec<u8>>> {
    let request = crate::api::client::with_request_timeout(
        crate::api::client::UREQ_AGENT
            .get(url)
            .header("User-Agent", "screen-goated-toolbox-updater"),
        Some(Duration::from_secs(10)),
    );
    match request.call() {
        Ok(response) => read_bounded(response, limit).map(Some),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(error) => Err(error).context("failed to fetch app update manifest"),
    }
}

fn fetch_required(url: &str, limit: u64) -> Result<Vec<u8>> {
    let response = crate::api::client::with_request_timeout(
        crate::api::client::UREQ_AGENT
            .get(url)
            .header("User-Agent", "screen-goated-toolbox-updater"),
        Some(Duration::from_secs(10)),
    )
    .call()
    .context("failed to fetch app update signature")?;
    read_bounded(response, limit)
}

fn read_bounded(response: ureq::http::Response<ureq::Body>, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("app update feed response exceeded its size limit");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_stable_contract() {
        let payload = br#"{"schemaVersion":1,"channel":"stable","version":"5.5.0","releaseNotes":"Notes","installer":{"name":"ScreenGoatedToolbox_v5.5.0.exe","url":"https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v5.5.0/ScreenGoatedToolbox_v5.5.0.exe","sizeBytes":123,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#;
        let candidate = parse(payload).unwrap();
        assert_eq!(candidate.version, semver::Version::new(5, 5, 0));
        assert_eq!(candidate.body, "Notes");
    }

    #[test]
    fn rejects_a_non_stable_channel() {
        let payload = br#"{"schemaVersion":1,"channel":"staging","version":"5.5.0","installer":{"name":"ScreenGoatedToolbox_v5.5.0.exe","url":"https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v5.5.0/ScreenGoatedToolbox_v5.5.0.exe","sizeBytes":123,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#;
        assert!(parse(payload).is_err());
    }
}
