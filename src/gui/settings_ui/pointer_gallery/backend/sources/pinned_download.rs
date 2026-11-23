//! Downloads that publish bytes only after they match a catalog pin.

use super::super::catalog::PinnedBytes;
use std::fs;
use std::io::Write as _;
use std::path::Path;

const USER_AGENT: &str = "screen-goated-toolbox-pointer-gallery";
const MAX_CURSOR_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

/// Streams `reader` into `output` and reports whether it held exactly the
/// pinned bytes. Growth past the pin stops the copy at once, so a hostile
/// source cannot fill the disk first.
fn matches_pin(
    reader: impl std::io::Read,
    output: &mut dyn std::io::Write,
    pin: PinnedBytes,
) -> Result<bool, String> {
    if pin.size > MAX_CURSOR_SOURCE_BYTES {
        return Err("Cursor source is larger than its pinned size".to_string());
    }
    crate::component_registry::stream_matches(
        reader,
        output,
        pin.size,
        pin.sha256,
        &|| false,
        &|_| {},
    )
    .map_err(|error| format!("Cursor source does not match its pinned size: {error}"))
}

/// True when an existing regular file holds exactly the pinned bytes.
pub(super) fn file_matches_pin(path: &Path, pin: PinnedBytes) -> Result<bool, String> {
    let file =
        fs::File::open(path).map_err(|error| format!("Failed to open {:?}: {error}", path))?;
    Ok(matches_pin(file, &mut std::io::sink(), pin).unwrap_or(false))
}

/// Reuses a cached archive only while it still matches its pin; anything else
/// is discarded and fetched again through the verified download.
pub(super) fn ensure_pinned_archive(
    url: &str,
    path: &Path,
    pin: PinnedBytes,
) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok() {
        super::validate_archive_file(path)?;
        if file_matches_pin(path, pin)? {
            return Ok(());
        }
        fs::remove_file(path)
            .map_err(|error| format!("Failed to discard unverified {:?}: {error}", path))?;
    }
    download_pinned(url, path, pin)
}

/// Streams `url` into a private temporary file and renames it into place only
/// after its size and SHA-256 equal the pin.
pub(super) fn download_pinned(
    url: &str,
    destination: &Path,
    pin: PinnedBytes,
) -> Result<(), String> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("Failed to download {url}: {error}"))?;
    let declared_bytes = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared_bytes.is_some_and(|bytes| bytes != pin.size) {
        return Err(format!(
            "Cursor source from {url} does not match its pinned size"
        ));
    }

    let temp_path = destination.with_extension("tmp");
    prepare_download_temp(&temp_path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| format!("Failed to create temp file {:?}: {error}", temp_path))?;
    let result = stream_verified(response.into_body().into_reader(), &mut file, pin)
        .map_err(|error| format!("{url}: {error}"));
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    fs::rename(&temp_path, destination).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Failed to move cursor file into place {:?}: {error}",
            destination
        )
    })
}

fn stream_verified(
    reader: impl std::io::Read,
    file: &mut fs::File,
    pin: PinnedBytes,
) -> Result<(), String> {
    let matched = matches_pin(reader, file, pin)?;
    file.flush()
        .map_err(|error| format!("Failed to flush download: {error}"))?;
    if !matched {
        return Err("Cursor source size or SHA-256 does not match its pin".to_string());
    }
    Ok(())
}

fn prepare_download_temp(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !metadata.is_file() || crate::component_registry::is_reparse_point(&metadata) {
        return Err(format!(
            "Cursor staging path {:?} is not a regular file",
            path
        ));
    }
    fs::remove_file(path).map_err(|error| format!("Failed to clear {:?}: {error}", path))
}

#[cfg(test)]
#[path = "pinned_download_tests.rs"]
mod tests;
