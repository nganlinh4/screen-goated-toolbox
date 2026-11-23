//! Re-pinning the delivery proof after an in-place edit.
//!
//! Every history mutation verifies the bytes on disk against the size, digest
//! and file identity captured when the result was delivered. An editor that
//! replaces the file therefore strands the entry: rename and delete answer
//! "Result file changed since it was recorded" until the proof is re-recorded.
//! Editors call [`refresh_artifact_proof`] immediately after replacing the file.

use super::*;

/// Re-pin the recorded artifact proof after a result is edited in place.
///
/// The mutex is taken without blocking: this runs on the webview IPC thread and
/// must not stall the UI behind a delivery publication. When the mutex is busy
/// the edited file is already durable, so a background thread repairs the proof
/// instead of the edit waiting for it.
pub fn refresh_artifact_proof(tool: &str, output_path: &str) -> Result<(), String> {
    validate_tool(tool)?;
    let path = history_path();
    match locking::try_acquire() {
        Ok(Some(_guard)) => refresh_at(&path, tool, output_path),
        Ok(None) | Err(_) => {
            let tool = tool.to_string();
            let output_path = output_path.to_string();
            std::thread::spawn(move || {
                let result =
                    locking::acquire().and_then(|_guard| refresh_at(&path, &tool, &output_path));
                if let Err(error) = result {
                    crate::log_info!(
                        "[Creation history] Artifact proof was not refreshed: {error}"
                    );
                }
            });
            Ok(())
        }
    }
}

/// Re-pins the recorded proof without the outer resume logic.
/// Direct use is experimental: [refresh_artifact_proof] routes and repairs it.
pub(crate) fn refresh_at(path: &Path, tool: &str, output_path: &str) -> Result<(), String> {
    let mut store = load_store(path)?;
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && same_path(&entry.output_path, output_path))
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let id = store.entries[index].id.clone();
    if store
        .pending_renames
        .iter()
        .any(|rename| rename.tool == tool && rename.entry_id == id)
        || store
            .pending_deletes
            .iter()
            .any(|delete| delete.entry.tool == tool && delete.entry.id == id)
    {
        return Err("Result is awaiting recovery.".to_string());
    }
    let artifact = inspect_delivery_artifact(output_path)?;
    store.entries[index].artifact_size_bytes = artifact.size_bytes;
    store.entries[index].artifact_sha256 = artifact.sha256;
    store.entries[index].managed_artifact = artifact.managed;
    if let Some(identity) = store
        .delivery_identities
        .iter_mut()
        .find(|identity| identity.tool == tool && identity.entry_id == id)
    {
        identity.artifact_file_identity = artifact.file_identity;
    }
    save_store(path, &store)
}
