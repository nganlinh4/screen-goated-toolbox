mod controller;
mod github;
mod lifecycle;
mod manifest;
#[cfg(test)]
mod parity_fixture;
#[cfg(test)]
mod restart_tests;
mod staging;

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

pub use controller::Updater;
pub use lifecycle::UpdateEvent;

const MAX_INSTALLER_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Eq, PartialEq)]
struct UpdateCandidate {
    version: semver::Version,
    body: String,
    asset_name: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone)]
pub struct StagedUpdate {
    version: String,
    asset_name: String,
    size_bytes: u64,
    sha256: String,
}

impl StagedUpdate {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn with_verified_path_beside<T>(
        &self,
        current_exe: &Path,
        launch: impl FnOnce(&Path) -> Result<T>,
    ) -> Result<T> {
        let version = semver::Version::parse(&self.version)?;
        let expected_name = format!("ScreenGoatedToolbox_v{version}.exe");
        if self.asset_name != expected_name {
            bail!("staged update identity no longer matches its checked version");
        }
        let path = current_exe
            .parent()
            .context("could not find executable directory")?
            .canonicalize()?
            .join(&self.asset_name);
        staging::with_locked_target(&path, |file| {
            verify_reader(file, self.size_bytes, &self.sha256)?;
            launch(&path)
        })
    }
}

impl UpdateCandidate {
    fn validate(&self) -> Result<()> {
        let expected_name = format!("ScreenGoatedToolbox_v{}.exe", self.version);
        if self.asset_name != expected_name {
            bail!("update installer name does not match its version");
        }
        let expected_url = format!(
            "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v{}/{}",
            self.version, expected_name
        );
        if self.download_url != expected_url {
            bail!("update installer URL is outside the stable release contract");
        }
        if self.size_bytes == 0 || self.size_bytes > MAX_INSTALLER_BYTES {
            bail!("update installer size is outside the accepted range");
        }
        if !self.version.pre.is_empty() || !self.version.build.is_empty() {
            bail!("update version is not a stable release");
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("update installer SHA-256 is invalid");
        }
        Ok(())
    }
}

fn latest_candidate() -> Result<UpdateCandidate> {
    latest_candidate_with(manifest::fetch, github::fetch_latest)
}

fn latest_candidate_with(
    primary: impl FnOnce() -> Result<Option<UpdateCandidate>>,
    fallback: impl FnOnce() -> Result<UpdateCandidate>,
) -> Result<UpdateCandidate> {
    match primary() {
        Ok(Some(candidate)) => Ok(candidate),
        Ok(None) => fallback().context("stable update feed is not published"),
        Err(error) => Err(error).context("stable update feed was rejected"),
    }
}

fn bump_is_greater(current: &str, new: &semver::Version) -> Result<bool> {
    Ok(new > &semver::Version::parse(current)?)
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate(String),
    UpdateAvailable { version: String, body: String },
    Downloading,
    Error(String),
    UpdatedAndRestartRequired(StagedUpdate),
}

fn download_and_stage(
    candidate: UpdateCandidate,
    should_stop: impl Fn() -> bool,
) -> Result<StagedUpdate> {
    candidate.validate()?;
    if !bump_is_greater(env!("CARGO_PKG_VERSION"), &candidate.version)? {
        bail!("the selected release is not newer than this app");
    }
    let exe_dir = std::env::current_exe()
        .context("could not get executable path")?
        .parent()
        .context("could not find executable directory")?
        .to_path_buf();
    stage_with(&candidate, &exe_dir, |file| {
        download_verified(&candidate, file, &should_stop)
    })
}

fn stage_with(
    candidate: &UpdateCandidate,
    exe_dir: &Path,
    download: impl FnOnce(&mut File) -> Result<()>,
) -> Result<StagedUpdate> {
    let staging_path = exe_dir.join(&candidate.asset_name);
    if !matching_staged_file(&staging_path, candidate)? {
        let mut scratch = staging::UpdateScratch::create(&staging_path)?;
        download(&mut scratch.file)?;
        scratch
            .file
            .sync_all()
            .context("failed to sync verified update")?;
        if let Err(error) = scratch.publish(&staging_path)
            && !matching_staged_file(&staging_path, candidate)?
        {
            return Err(error);
        }
    }
    Ok(StagedUpdate {
        version: candidate.version.to_string(),
        asset_name: candidate.asset_name.clone(),
        size_bytes: candidate.size_bytes,
        sha256: candidate.sha256.clone(),
    })
}

fn matching_staged_file(path: &Path, candidate: &UpdateCandidate) -> Result<bool> {
    match File::open(path) {
        Ok(file) => {
            verify_reader(file, candidate.size_bytes, &candidate.sha256)
                .context("existing update filename contains different or unverifiable data")?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("failed to inspect existing update filename"),
    }
}

/// The download agent bounds connection, headers and each idle body read, but
/// deliberately has no whole-body deadline: a slow link must still finish a
/// large installer. Cancellation is checked between reads instead.
fn download_verified(
    candidate: &UpdateCandidate,
    destination: &mut File,
    should_stop: impl Fn() -> bool,
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(&candidate.download_url)
        .call()
        .context("failed to download update")?;
    let mut reader = response.into_body().into_reader();
    copy_verified(&mut reader, destination, candidate, should_stop)
}

fn copy_verified(
    reader: impl Read,
    mut writer: impl Write,
    candidate: &UpdateCandidate,
    should_stop: impl Fn() -> bool,
) -> Result<()> {
    if candidate.size_bytes > MAX_INSTALLER_BYTES {
        bail!("downloaded update exceeded its declared size");
    }
    let matched = crate::component_registry::stream_matches(
        reader,
        &mut writer,
        candidate.size_bytes,
        &candidate.sha256,
        &should_stop,
        &|_| {},
    )
    .context("update download was interrupted")?;
    if !matched {
        bail!("downloaded update failed size or SHA-256 verification");
    }
    Ok(())
}

fn verify_reader(reader: impl Read, expected_size: u64, expected_sha256: &str) -> Result<()> {
    if expected_size > MAX_INSTALLER_BYTES {
        bail!("staged update exceeded its verified size");
    }
    let matched = crate::component_registry::stream_matches(
        reader,
        &mut std::io::sink(),
        expected_size,
        expected_sha256,
        &|| false,
        &|_| {},
    )?;
    if !matched {
        bail!("staged update changed after verification");
    }
    Ok(())
}

#[cfg(test)]
#[path = "updater/staging_tests.rs"]
mod staging_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest as _, Sha256};

    fn valid_candidate(bytes: &[u8]) -> UpdateCandidate {
        UpdateCandidate {
            version: semver::Version::new(5, 5, 0),
            body: String::new(),
            asset_name: "ScreenGoatedToolbox_v5.5.0.exe".into(),
            download_url: "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v5.5.0/ScreenGoatedToolbox_v5.5.0.exe".into(),
            size_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }

    #[test]
    fn candidate_rejects_mismatched_asset_identity() {
        let candidate = UpdateCandidate {
            version: semver::Version::new(5, 5, 0),
            body: String::new(),
            asset_name: "ScreenGoatedToolbox_v5.4.3.exe".into(),
            download_url: "https://example.com/update.exe".into(),
            size_bytes: 100,
            sha256: "a".repeat(64),
        };
        assert!(candidate.validate().is_err());
    }

    #[test]
    fn verified_copy_rejects_truncation_and_digest_mismatch() {
        let bytes = b"complete installer";
        let candidate = valid_candidate(bytes);
        assert!(copy_verified(&bytes[..5], Vec::new(), &candidate, || false).is_err());

        let mut wrong_digest = candidate;
        wrong_digest.sha256 = "0".repeat(64);
        assert!(copy_verified(bytes.as_slice(), Vec::new(), &wrong_digest, || false).is_err());
    }

    #[test]
    fn verified_copy_accepts_exact_bytes() {
        let bytes = b"complete installer";
        let candidate = valid_candidate(bytes);
        let mut output = Vec::new();
        copy_verified(bytes.as_slice(), &mut output, &candidate, || false).unwrap();
        assert_eq!(output, bytes);
    }

    #[test]
    fn verified_copy_stops_between_reads_once_cancelled() {
        let bytes = vec![7_u8; 3 * 1024 * 1024];
        let candidate = valid_candidate(&bytes);
        let reads = std::cell::Cell::new(0_usize);
        let mut output = Vec::new();
        let error = copy_verified(bytes.as_slice(), &mut output, &candidate, || {
            reads.set(reads.get() + 1);
            reads.get() > 1
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("cancelled"), "{error:#}");
        assert!(!output.is_empty() && output.len() < bytes.len());
    }

    #[test]
    fn restart_path_is_exact_and_reverified_after_staging() {
        let bytes = b"verified executable";
        let candidate = valid_candidate(bytes);
        let directory = std::env::temp_dir().join(format!(
            "sgt-updater-restart-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let current_exe = directory.join("current.exe");
        let staged_path = directory.join(&candidate.asset_name);
        let untouched = [
            directory.join("ScreenGoatedToolbox_v1.0.0.exe"),
            directory.join("other.exe.old"),
            directory.join("update_pending.exe"),
            directory.join("temp_download"),
            directory.join("sgt_restart_123.bat"),
        ];
        for path in &untouched {
            std::fs::write(path, b"preserve").unwrap();
        }
        std::fs::write(&staged_path, bytes).unwrap();
        let staged = StagedUpdate {
            version: candidate.version.to_string(),
            asset_name: candidate.asset_name,
            size_bytes: candidate.size_bytes,
            sha256: candidate.sha256,
        };

        assert_eq!(
            staged
                .with_verified_path_beside(&current_exe, |path| Ok(path.to_owned()))
                .unwrap(),
            staged_path.canonicalize().unwrap()
        );
        std::fs::write(&staged_path, b"changed executable").unwrap();
        assert!(
            staged
                .with_verified_path_beside(&current_exe, |path| Ok(path.to_owned()))
                .is_err()
        );
        for path in &untouched {
            assert_eq!(std::fs::read(path).unwrap(), b"preserve");
            std::fs::remove_file(path).unwrap();
        }

        std::fs::remove_file(staged_path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
