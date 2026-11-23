//! File dialogs used by the TTS Playground (pick a source audio file, save a
//! WAV / MP3). Promoted here from the now-retiring egui module so the WRY
//! mini-app + any other callers don't have to depend on the old GUI tree.

use std::path::PathBuf;

/// Picks an existing audio file (WAV / FLAC / OGG / AIFF…) via the Windows
/// system open-file dialog. Returns `Ok(None)` when the user cancels.
pub fn pick_audio_file_dialog() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_audio_file_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

/// Writes the WAV bytes for a TTS Playground clip to a user-chosen path and
/// returns the path. Cancel returns Err("Save cancelled") so callers can
/// distinguish cancel from real I/O errors.
#[cfg(not(feature = "recorder-worker"))]
pub fn save_wav(default_filename: &str, wav_bytes: &[u8]) -> Result<PathBuf, String> {
    let path = save_file_dialog(default_filename, "WAV Audio (*.wav)", "*.wav", "wav")?;
    std::fs::write(&path, wav_bytes).map_err(|err| err.to_string())?;
    Ok(path)
}

/// Transcodes WAV bytes to MP3 using ffmpeg and writes the result to a
/// user-chosen path. Uses the FFmpeg dependency manager so an automatic
/// download happens if missing.
#[cfg(not(feature = "recorder-worker"))]
pub fn save_mp3(default_filename: &str, wav_bytes: &[u8], clip_id: u64) -> Result<PathBuf, String> {
    let ffmpeg =
        crate::gui::settings_ui::download_manager::ffmpeg_dependency::acquire_ffmpeg_with_badge()?;
    let output_path = save_file_dialog(default_filename, "MP3 Audio (*.mp3)", "*.mp3", "mp3")?;
    let temp_wav = std::env::temp_dir().join(format!("sgt_tts_playground_{clip_id}.wav"));
    std::fs::write(&temp_wav, wav_bytes).map_err(|err| err.to_string())?;

    let output = crate::system_command::background_command(ffmpeg.executable())
        .args([
            "-y",
            "-i",
            temp_wav.to_str().unwrap_or(""),
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "192k",
            output_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|err| format!("Failed to launch FFmpeg: {err}"))?;

    let _ = std::fs::remove_file(&temp_wav);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg MP3 export failed: {stderr}"));
    }
    Ok(output_path)
}

#[cfg(not(feature = "recorder-worker"))]
fn save_file_dialog(
    default_name: &str,
    filter_name: &str,
    filter_pattern: &str,
    default_ext: &str,
) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        save_file_dialog_windows(default_name, filter_name, filter_pattern, default_ext)
    }
    #[cfg(not(windows))]
    {
        let path = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(default_name);
        let _ = (filter_name, filter_pattern, default_ext);
        Ok(path)
    }
}

#[cfg(windows)]
fn pick_audio_file_dialog_windows() -> Result<Option<PathBuf>, String> {
    use crate::native_file_dialog::{Filter, Kind, Options, select};
    use windows::Win32::UI::Shell::FOLDERID_Music;
    select(
        None,
        &Options {
            kind: Kind::Files,
            filters: &[
                Filter {
                    label: "Audio files",
                    pattern: "*.wav;*.flac;*.ogg;*.aiff;*.aif",
                },
                Filter {
                    label: "All files",
                    pattern: "*.*",
                },
            ],
            default_folder: &FOLDERID_Music,
            initial_folder: None,
            initial_name: None,
            default_extension: None,
            maximum_selections: 1,
        },
    )
    .map(|mut paths| paths.pop())
    .map_err(|error| error.to_string())
}

#[cfg(all(windows, not(feature = "recorder-worker")))]
fn save_file_dialog_windows(
    default_name: &str,
    filter_name: &str,
    filter_pattern: &str,
    default_ext: &str,
) -> Result<PathBuf, String> {
    use crate::native_file_dialog::{Filter, Kind, Options, select};
    use windows::Win32::UI::Shell::FOLDERID_Downloads;
    select(
        None,
        &Options {
            kind: Kind::Save,
            initial_name: Some(default_name),
            default_extension: Some(default_ext),
            filters: &[Filter {
                label: filter_name,
                pattern: filter_pattern,
            }],
            default_folder: &FOLDERID_Downloads,
            initial_folder: None,
            maximum_selections: 1,
        },
    )
    .map_err(|error| error.to_string())?
    .pop()
    .ok_or_else(|| "Save cancelled".to_string())
}
