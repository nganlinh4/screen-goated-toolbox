use super::UpdateCandidate;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const RELEASES_PER_PAGE: usize = 100;
const MAX_RELEASE_PAGES: usize = 10;
const MAX_RELEASE_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

pub(super) fn fetch_latest() -> Result<UpdateCandidate> {
    let mut candidates = Vec::new();
    for page in 1..=MAX_RELEASE_PAGES {
        let url = format!(
            "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases?per_page={RELEASES_PER_PAGE}&page={page}"
        );
        let response = crate::api::client::with_request_timeout(
            crate::api::client::UREQ_AGENT
                .get(&url)
                .header("User-Agent", "screen-goated-toolbox-updater"),
            Some(Duration::from_secs(10)),
        )
        .call()
        .context("failed to fetch GitHub stable releases")?;
        let mut body = String::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_RELEASE_RESPONSE_BYTES + 1)
            .read_to_string(&mut body)
            .context("failed to read GitHub release response")?;
        if body.len() as u64 > MAX_RELEASE_RESPONSE_BYTES {
            anyhow::bail!("GitHub release response exceeded its size limit");
        }
        let releases: Vec<GithubRelease> =
            serde_json::from_str(&body).context("GitHub release response was invalid")?;
        let count = releases.len();
        candidates.extend(select_candidates(releases));
        if count < RELEASES_PER_PAGE {
            break;
        }
    }
    candidates
        .into_iter()
        .max_by(|left, right| left.version.cmp(&right.version))
        .context("no compatible stable GitHub release was found")
}

fn select_candidates(releases: Vec<GithubRelease>) -> Vec<UpdateCandidate> {
    releases
        .into_iter()
        .filter_map(candidate_from_release)
        .collect()
}

fn candidate_from_release(release: GithubRelease) -> Option<UpdateCandidate> {
    if release.draft || release.prerelease {
        return None;
    }
    let version = release
        .tag_name
        .strip_prefix('v')
        .and_then(|value| semver::Version::parse(value).ok())?;
    let expected_name = format!("ScreenGoatedToolbox_v{version}.exe");
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == expected_name)?;
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_owned)?;
    let candidate = UpdateCandidate {
        version,
        body: release.body.unwrap_or_default(),
        asset_name: asset.name,
        download_url: asset.browser_download_url,
        size_bytes: asset.size,
        sha256,
    };
    candidate.validate().ok().map(|()| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, asset_version: &str) -> GithubRelease {
        let name = format!("ScreenGoatedToolbox_v{asset_version}.exe");
        GithubRelease {
            tag_name: tag.into(),
            body: Some(String::new()),
            draft: false,
            prerelease,
            assets: vec![GithubAsset {
                browser_download_url: format!(
                    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/{tag}/{name}"
                ),
                name,
                size: 123,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            }],
        }
    }

    #[test]
    fn ignores_staging_and_malformed_releases() {
        let candidates = select_candidates(vec![
            release("sgt-runtime-staging", false, "5.4.3"),
            release("v9.0.0", true, "9.0.0"),
            release("v5.4.3", false, "5.4.3"),
        ]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].version, semver::Version::new(5, 4, 3));
    }

    #[test]
    fn stable_candidates_are_order_independent() {
        let candidates = select_candidates(vec![
            release("v5.4.3", false, "5.4.3"),
            release("v5.5.0", false, "5.5.0"),
            release("v5.4.2", false, "5.4.2"),
        ]);
        let latest = candidates
            .into_iter()
            .max_by(|left, right| left.version.cmp(&right.version))
            .unwrap();
        assert_eq!(latest.version, semver::Version::new(5, 5, 0));
    }

    #[test]
    fn rejects_an_asset_from_a_different_version() {
        assert!(candidate_from_release(release("v5.5.0", false, "5.4.3")).is_none());
    }
}
