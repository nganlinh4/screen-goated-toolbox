//! Debug/test-only file snapshots at scripted turn boundaries.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};

use super::super::telemetry::{self, Privacy};
use publication::{PublicationError, RootLock, StagingTurn};
use stability::{
    ConfiguredSource, LockedSource, SnapshotEvidence, StabilityError, probe_sources, settle_sources,
};

mod publication;
mod stability;
#[cfg(test)]
mod tests;

const PATHS_ENV: &str = "CC_SCRIPTED_SNAPSHOT_PATHS_JSON";
const DIR_ENV: &str = "CC_SCRIPTED_SNAPSHOT_DIR";

pub(super) struct ScriptedSnapshots {
    sources: Vec<ConfiguredSource>,
    root: PathBuf,
    root_lock: RootLock,
}

impl ScriptedSnapshots {
    pub(super) fn from_environment() -> anyhow::Result<Option<Self>> {
        Self::from_values(std::env::var_os(PATHS_ENV), std::env::var_os(DIR_ENV))
    }

    fn from_values(
        paths_json: Option<OsString>,
        destination: Option<OsString>,
    ) -> anyhow::Result<Option<Self>> {
        let (paths_json, destination) = match (paths_json, destination) {
            (None, None) => return Ok(None),
            (Some(paths_json), Some(destination)) => (paths_json, destination),
            _ => anyhow::bail!("{PATHS_ENV} and {DIR_ENV} must be configured together"),
        };
        let paths_json = paths_json
            .into_string()
            .map_err(|_| anyhow::anyhow!("{PATHS_ENV} must contain Unicode JSON"))?;
        let raw_paths: Vec<String> = serde_json::from_str(&paths_json)
            .with_context(|| format!("{PATHS_ENV} must be a JSON string array"))?;
        ensure!(!raw_paths.is_empty(), "{PATHS_ENV} must not be empty");

        let mut sources = Vec::with_capacity(raw_paths.len());
        for (index, raw_path) in raw_paths.into_iter().enumerate() {
            sources.push(
                ConfiguredSource::configure(PathBuf::from(raw_path)).with_context(|| {
                    format!("{PATHS_ENV} item {} is not a canonical file", index + 1)
                })?,
            );
        }

        let root = PathBuf::from(destination);
        ensure!(root.is_absolute(), "{DIR_ENV} must be an absolute path");
        std::fs::create_dir(&root)
            .with_context(|| format!("{DIR_ENV} must identify a new directory"))?;
        let root = std::fs::canonicalize(&root)
            .with_context(|| format!("{DIR_ENV} could not be canonicalized"))?;
        let root_lock = RootLock::acquire(&root)
            .map_err(|error| anyhow::Error::new(error).context("snapshot root lock failed"))?;
        Ok(Some(Self {
            sources,
            root,
            root_lock,
        }))
    }

    pub(super) fn capture_turn(&self, turn_index: usize) -> anyhow::Result<()> {
        ensure!(turn_index > 0, "snapshot turn index must be positive");
        let relative_dir = PathBuf::from(format!("turn-{turn_index:04}"));
        let turn_dir = self.root.join(&relative_dir);
        self.root_lock
            .authenticate()
            .map_err(|error| publication_failure(turn_index, error))?;

        let baselines = probe_sources(&self.sources)
            .map_err(|error| stability_failure(turn_index, None, "initial_probe", error))?;
        settle_sources();
        let mut locked = LockedSource::acquire_all(&self.sources, &baselines)
            .map_err(|error| stability_failure(turn_index, None, "source_set_lock", error))?;
        let staging =
            StagingTurn::new(&self.root).map_err(|error| publication_failure(turn_index, error))?;
        let mut artifacts = Vec::with_capacity(locked.len());
        let mut evidence = Vec::with_capacity(locked.len());

        for (offset, source) in locked.iter_mut().enumerate() {
            let file_index = offset + 1;
            let relative = relative_dir.join(format!("file-{file_index:04}.snapshot"));
            let (artifact, captured) =
                source
                    .stage(&staging.artifact_path(file_index))
                    .map_err(|error| {
                        stability_failure(turn_index, Some(file_index), "stable_copy", error)
                    })?;
            artifacts.push(artifact);
            evidence.push((
                file_index,
                source.configured_path().to_path_buf(),
                relative,
                captured,
            ));
        }

        for (offset, (source, (_, _, _, captured))) in locked.iter_mut().zip(&evidence).enumerate()
        {
            source.authenticate(captured).map_err(|error| {
                stability_failure(
                    turn_index,
                    Some(offset + 1),
                    "locked_set_authentication",
                    error,
                )
            })?;
        }

        let published_guard = staging
            .publish(artifacts, &turn_dir)
            .map_err(|error| publication_failure(turn_index, error))?;
        self.root_lock
            .authenticate()
            .map_err(|error| publication_failure(turn_index, error))?;

        for (file_index, source, relative, captured) in &evidence {
            record_evidence(turn_index, *file_index, source, relative, captured);
        }
        telemetry::event(
            "scripted_snapshot_turn_complete",
            "test_harness",
            Privacy::Safe,
            serde_json::json!({
                "scripted_turn_index": turn_index,
                "file_count": self.sources.len(),
                "destination_directory_created": true,
                "atomic_publish": true,
                "post_publish_authenticated": true,
            }),
        );
        drop(published_guard);
        drop(locked);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn for_test(sources: Vec<PathBuf>, root: PathBuf) -> anyhow::Result<Self> {
        let paths_json = serde_json::to_string(&sources).expect("test paths serialize");
        Self::from_values(Some(paths_json.into()), Some(root.into()))?
            .context("test snapshot configuration unexpectedly disabled")
    }
}

fn stability_failure(
    turn_index: usize,
    file_index: Option<usize>,
    phase: &'static str,
    error: StabilityError,
) -> anyhow::Error {
    telemetry::typed_error(
        "ERR_SCRIPTED_SNAPSHOT_SOURCE_UNSTABLE",
        "test_harness",
        "configured snapshot source failed the stable publication boundary",
        serde_json::json!({
            "scripted_turn_index": turn_index,
            "file_index": file_index,
            "phase": phase,
            "reason": error.reason,
        }),
    );
    anyhow::Error::new(error).context(format!("snapshot source stability failed during {phase}"))
}

fn publication_failure(turn_index: usize, error: PublicationError) -> anyhow::Error {
    telemetry::typed_error(
        "ERR_SCRIPTED_SNAPSHOT_PUBLICATION_FAILED",
        "test_harness",
        "scripted snapshot could not be published without clobbering evidence",
        serde_json::json!({
            "scripted_turn_index": turn_index,
            "reason": error.reason,
        }),
    );
    anyhow::Error::new(error).context("scripted snapshot publication failed")
}

fn record_evidence(
    turn_index: usize,
    file_index: usize,
    source: &Path,
    relative: &Path,
    evidence: &SnapshotEvidence,
) {
    telemetry::event(
        "scripted_snapshot_metadata",
        "test_harness",
        Privacy::Safe,
        serde_json::json!({
            "scripted_turn_index": turn_index,
            "file_index": file_index,
            "byte_count": evidence.bytes,
            "sha256_recorded": true,
            "destination_created": true,
        }),
    );
    telemetry::event(
        "scripted_snapshot_evidence",
        "test_harness",
        Privacy::Sensitive,
        serde_json::json!({
            "scripted_turn_index": turn_index,
            "file_index": file_index,
            "byte_count": evidence.bytes,
            "sha256": evidence.sha256,
            "source_path": source.to_string_lossy(),
            "destination_relative": relative.to_string_lossy(),
        }),
    );
}
