mod github;
mod manifest;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::time::Duration;

const MAX_INSTALLER_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
struct UpdateCandidate {
    version: semver::Version,
    body: String,
    asset_name: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Default)]
struct UpdateSelection {
    generation: u64,
    candidate: Option<UpdateCandidate>,
}

impl UpdateSelection {
    fn begin_check(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.candidate = None;
        self.generation
    }
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

    pub(crate) fn verified_path_beside(&self, current_exe: &Path) -> Result<PathBuf> {
        let version = semver::Version::parse(&self.version)?;
        let expected_name = format!("ScreenGoatedToolbox_v{version}.exe");
        if self.asset_name != expected_name {
            bail!("staged update identity no longer matches its checked version");
        }
        let path = current_exe
            .parent()
            .context("could not find executable directory")?
            .join(&self.asset_name);
        let file = std::fs::File::open(&path).context("verified staged update is missing")?;
        verify_reader(file, self.size_bytes, &self.sha256)?;
        Ok(path)
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
    match manifest::fetch() {
        Ok(Some(candidate)) => Ok(candidate),
        Ok(None) => github::fetch_latest().context("stable update feed is not published"),
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

pub struct Updater {
    tx: Sender<UpdateStatus>,
    selection: Arc<Mutex<UpdateSelection>>,
    check_task: Mutex<Option<crate::task_runtime::TaskTicket>>,
    download_task: Mutex<Option<crate::task_runtime::TaskTicket>>,
}

impl Updater {
    pub fn new(tx: Sender<UpdateStatus>) -> Self {
        Self {
            tx,
            selection: Arc::new(Mutex::new(UpdateSelection::default())),
            check_task: Mutex::new(None),
            download_task: Mutex::new(None),
        }
    }

    pub fn check_for_updates(&self) {
        replace_task(&self.check_task, None);
        let tx = self.tx.clone();
        let selection = Arc::clone(&self.selection);
        let generation = {
            let Ok(mut state) = selection.lock() else {
                send_error(&tx, "update selection state was unavailable");
                return;
            };
            let generation = state.begin_check();
            let _ = tx.send(UpdateStatus::Checking);
            generation
        };
        let queued = crate::task_runtime::spawn(
            crate::task_runtime::TaskClass::Interactive,
            "update-check",
            move |context| {
                crate::log_info!("[Updater] check_started task={}", context.id());
                match latest_candidate() {
                    Ok(candidate) => {
                        let current = env!("CARGO_PKG_VERSION");
                        match bump_is_greater(current, &candidate.version) {
                            Ok(true) => {
                                let status = UpdateStatus::UpdateAvailable {
                                    version: candidate.version.to_string(),
                                    body: candidate.body.clone(),
                                };
                                publish_check_result(
                                    &selection,
                                    generation,
                                    &tx,
                                    status,
                                    Some(candidate),
                                );
                            }
                            Ok(false) => {
                                publish_check_result(
                                    &selection,
                                    generation,
                                    &tx,
                                    UpdateStatus::UpToDate(current.to_string()),
                                    None,
                                );
                            }
                            Err(error) => publish_check_result(
                                &selection,
                                generation,
                                &tx,
                                UpdateStatus::Error(error.to_string()),
                                None,
                            ),
                        }
                    }
                    Err(error) => publish_check_result(
                        &selection,
                        generation,
                        &tx,
                        UpdateStatus::Error(error.to_string()),
                        None,
                    ),
                }
            },
        );
        match queued {
            Ok(ticket) => replace_task(&self.check_task, Some(ticket)),
            Err(error) => send_error(&self.tx, error),
        }
    }

    pub fn perform_update(&self) {
        replace_task(&self.download_task, None);
        let tx = self.tx.clone();
        let candidate = self
            .selection
            .lock()
            .ok()
            .and_then(|mut state| state.candidate.take());
        let Some(candidate) = candidate else {
            send_error(&tx, "check for updates again before downloading");
            return;
        };
        let _ = tx.send(UpdateStatus::Downloading);
        let queued = crate::task_runtime::spawn(
            crate::task_runtime::TaskClass::Io,
            "update-download",
            move |context| {
                crate::log_info!("[Updater] download_started task={}", context.id());
                match download_and_stage(candidate) {
                    Ok(staged) => {
                        let _ = tx.send(UpdateStatus::UpdatedAndRestartRequired(staged));
                    }
                    Err(error) => send_error(&tx, error),
                }
            },
        );
        match queued {
            Ok(ticket) => replace_task(&self.download_task, Some(ticket)),
            Err(error) => send_error(&self.tx, error),
        }
    }
}

fn replace_task(
    slot: &Mutex<Option<crate::task_runtime::TaskTicket>>,
    replacement: Option<crate::task_runtime::TaskTicket>,
) {
    if let Ok(mut current) = slot.lock() {
        if let Some(task) = current.take() {
            crate::log_info!("[Updater] cancelling_superseded task={}", task.id());
            task.cancel();
        }
        *current = replacement;
    }
}

fn publish_check_result(
    selection: &Mutex<UpdateSelection>,
    generation: u64,
    tx: &Sender<UpdateStatus>,
    status: UpdateStatus,
    candidate: Option<UpdateCandidate>,
) {
    let Ok(mut state) = selection.lock() else {
        send_error(tx, "update selection state was unavailable");
        return;
    };
    if state.generation != generation {
        return;
    }
    state.candidate = candidate;
    let _ = tx.send(status);
}

fn download_and_stage(candidate: UpdateCandidate) -> Result<StagedUpdate> {
    candidate.validate()?;
    if !bump_is_greater(env!("CARGO_PKG_VERSION"), &candidate.version)? {
        bail!("the selected release is not newer than this app");
    }
    let exe_dir = std::env::current_exe()
        .context("could not get executable path")?
        .parent()
        .context("could not find executable directory")?
        .to_path_buf();
    let partial_path = exe_dir.join("update_download.part");
    let staging_path = exe_dir.join(&candidate.asset_name);
    if partial_path.exists() {
        std::fs::remove_file(&partial_path).context("failed to clear an older partial update")?;
    }

    let result = download_verified(&candidate, &partial_path)
        .and_then(|()| replace_staged_file(&partial_path, &staging_path));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial_path);
    }
    result?;
    Ok(StagedUpdate {
        version: candidate.version.to_string(),
        asset_name: candidate.asset_name,
        size_bytes: candidate.size_bytes,
        sha256: candidate.sha256,
    })
}

fn download_verified(candidate: &UpdateCandidate, destination: &Path) -> Result<()> {
    let response = crate::api::client::with_request_timeout(
        crate::api::client::UREQ_DOWNLOAD_AGENT.get(&candidate.download_url),
        Some(Duration::from_secs(5 * 60)),
    )
    .call()
    .context("failed to download update")?;
    let mut reader = response.into_body().into_reader();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .context("failed to create update staging file")?;
    copy_verified(&mut reader, &mut file, candidate)?;
    file.sync_all()?;
    Ok(())
}

fn copy_verified(
    mut reader: impl Read,
    mut writer: impl Write,
    candidate: &UpdateCandidate,
) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .context("update download was interrupted")?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > candidate.size_bytes || total > MAX_INSTALLER_BYTES {
            bail!("downloaded update exceeded its declared size");
        }
        writer.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
    }
    if total != candidate.size_bytes {
        bail!("downloaded update size did not match its release contract");
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(&candidate.sha256) {
        bail!("downloaded update failed SHA-256 verification");
    }
    Ok(())
}

fn verify_reader(mut reader: impl Read, expected_size: u64, expected_sha256: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > expected_size || total > MAX_INSTALLER_BYTES {
            bail!("staged update exceeded its verified size");
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_size {
        bail!("staged update size changed after verification");
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        bail!("staged update SHA-256 changed after verification");
    }
    Ok(())
}

fn replace_staged_file(partial: &Path, staging: &Path) -> Result<()> {
    if staging.exists() {
        std::fs::remove_file(staging).context("failed to replace an older staged update")?;
    }
    std::fs::rename(partial, staging).context("failed to stage the verified update")
}

fn send_error(tx: &Sender<UpdateStatus>, error: impl std::fmt::Display) {
    let _ = tx.send(UpdateStatus::Error(error.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(copy_verified(&bytes[..5], Vec::new(), &candidate).is_err());

        let mut wrong_digest = candidate;
        wrong_digest.sha256 = "0".repeat(64);
        assert!(copy_verified(bytes.as_slice(), Vec::new(), &wrong_digest).is_err());
    }

    #[test]
    fn verified_copy_accepts_exact_bytes() {
        let bytes = b"complete installer";
        let candidate = valid_candidate(bytes);
        let mut output = Vec::new();
        copy_verified(bytes.as_slice(), &mut output, &candidate).unwrap();
        assert_eq!(output, bytes);
    }

    #[test]
    fn stale_check_generation_cannot_publish_or_replace_selection() {
        let (tx, rx) = std::sync::mpsc::channel();
        let selection = Mutex::new(UpdateSelection {
            generation: 2,
            candidate: None,
        });
        publish_check_result(
            &selection,
            1,
            &tx,
            UpdateStatus::Error("stale".into()),
            Some(valid_candidate(b"stale installer")),
        );

        assert!(rx.try_recv().is_err());
        assert!(selection.lock().unwrap().candidate.is_none());
    }

    #[test]
    fn selected_candidate_can_be_consumed_only_once() {
        let mut selection = UpdateSelection {
            generation: 1,
            candidate: Some(valid_candidate(b"installer")),
        };
        assert!(selection.candidate.take().is_some());
        assert!(selection.candidate.take().is_none());
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
        std::fs::write(&staged_path, bytes).unwrap();
        let staged = StagedUpdate {
            version: candidate.version.to_string(),
            asset_name: candidate.asset_name,
            size_bytes: candidate.size_bytes,
            sha256: candidate.sha256,
        };

        assert_eq!(
            staged.verified_path_beside(&current_exe).unwrap(),
            staged_path
        );
        std::fs::write(&staged_path, b"changed executable").unwrap();
        assert!(staged.verified_path_beside(&current_exe).is_err());

        std::fs::remove_file(staged_path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
