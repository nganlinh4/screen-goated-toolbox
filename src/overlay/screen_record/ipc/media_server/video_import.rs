//! One import routine for video files, shared by the HTTP upload and the
//! native-path (drag and drop) IPC command.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::overlay::screen_record::owned_file::OwnedFile;

use super::media_probe::probe_media_has_audio;
use super::{recordings_dir, unique_import_stamp};

const VIDEO_IMPORT_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi"];

pub(super) struct ImportedVideo {
    pub(super) path: PathBuf,
    pub(super) has_audio: bool,
    pub(super) bytes: u64,
}

pub(super) fn video_import_extension(file_name: &str) -> Result<&'static str, String> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    VIDEO_IMPORT_EXTENSIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == extension)
        .ok_or_else(|| format!("Unsupported video file type: .{extension}"))
}

/// Fills a fresh managed file with `fill`, then probes it. The managed copy is
/// removed on any failure, so an unreadable import never leaves an orphan.
pub(super) fn import_video_into_managed(
    extension: &'static str,
    fill: impl FnOnce(&Path) -> Result<u64, String>,
) -> Result<ImportedVideo, String> {
    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let output = OwnedFile::new(
        recordings_dir.join(format!("imported-{}.{extension}", unique_import_stamp())),
    );
    let bytes = fill(output.path())?;
    let has_audio = probe_media_has_audio(output.path())?;
    Ok(ImportedVideo {
        path: output.keep(),
        has_audio,
        bytes,
    })
}

pub fn import_video_path_to_managed_media_file(
    source_path: &Path,
    trace_id: &str,
) -> Result<(String, bool), String> {
    if !source_path.is_file() {
        return Err(format!("Video file not found: {}", source_path.display()));
    }
    let extension = video_import_extension(&source_path.to_string_lossy())?;
    let started_at = Instant::now();
    let imported = import_video_into_managed(extension, |output| {
        std::fs::copy(source_path, output).map_err(|error| {
            format!(
                "Copy imported video failed from '{}' to '{}': {error}",
                source_path.display(),
                output.display()
            )
        })
    })?;
    crate::log_info!(
        "[VideoImport:{}][Path] complete total {:.3}s file=\"{}\" output=\"{}\" has_audio={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        source_path.display(),
        imported.path.display(),
        imported.has_audio
    );
    Ok((
        imported.path.to_string_lossy().to_string(),
        imported.has_audio,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_allow_list_is_shared_and_case_insensitive() {
        assert_eq!(video_import_extension("Clip.MKV").unwrap(), "mkv");
        assert_eq!(video_import_extension("a.b.webm").unwrap(), "webm");
        assert!(video_import_extension("notes.txt").is_err());
        assert!(video_import_extension("no-extension").is_err());
    }

    #[test]
    fn unreadable_import_leaves_no_managed_copy() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let source = std::env::temp_dir().join(format!(
            "sgt-video-import-{}-{}.mp4",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        std::fs::write(&source, b"not a media file").unwrap();
        let before = managed_imports();
        let error = import_video_path_to_managed_media_file(&source, "test").unwrap_err();
        assert!(!error.is_empty());
        assert_eq!(
            managed_imports(),
            before,
            "probe failure must remove the copy"
        );
        std::fs::remove_file(source).unwrap();
    }

    fn managed_imports() -> Vec<std::path::PathBuf> {
        let mut entries: Vec<_> = std::fs::read_dir(recordings_dir())
            .map(|dir| {
                dir.flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| {
                                name.starts_with("imported-") && !name.starts_with("imported-audio")
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort();
        entries
    }
}
