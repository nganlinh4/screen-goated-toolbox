//! Shared helpers for the persistent Python TTS sidecars (Step Audio, VieNeu,
//! Magpie). Only the genuinely-identical plumbing lives here; each worker keeps
//! its own request/response structs and run loop because their process
//! lifecycles (cancellation model, stderr capture, spawn command) differ.

use anyhow::{Context, Result, bail};

/// Verifies that a sidecar response carries the request id it was issued for.
/// Identical across every sidecar worker; only the provider name differs.
pub(super) fn check_response_id(provider: &str, request_id: &str, response_id: &str) -> Result<()> {
    if !response_id.is_empty() && response_id != request_id {
        bail!("{provider} sidecar response id mismatch: expected {request_id}, got {response_id}");
    }
    Ok(())
}

/// Builds the per-request temp WAV path shared by the Step Audio and VieNeu
/// sidecars: `<temp>/screen-goated-toolbox/tts/<prefix>-<req_id>.wav`.
pub(super) fn temp_wav_path(prefix: &str, req_id: u64) -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir()
        .join("screen-goated-toolbox")
        .join("tts");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create temp dir '{}'", dir.display()))?;
    Ok(dir.join(format!("{prefix}-{req_id}.wav")))
}
