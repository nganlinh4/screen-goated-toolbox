//! File dialogs used by the TTS Playground (pick a source audio file, save a
//! WAV / MP3). Promoted here from the now-retiring egui module so the WRY
//! mini-app + any other callers don't have to depend on the old GUI tree.

use std::path::{Path, PathBuf};

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
pub fn save_wav(default_filename: &str, wav_bytes: &[u8]) -> Result<PathBuf, String> {
    let path = save_file_dialog(default_filename, "WAV Audio (*.wav)", "*.wav", "wav")?;
    std::fs::write(&path, wav_bytes).map_err(|err| err.to_string())?;
    Ok(path)
}

/// Transcodes WAV bytes to MP3 using ffmpeg and writes the result to a
/// user-chosen path. Uses the FFmpeg dependency manager so an automatic
/// download happens if missing.
pub fn save_mp3(default_filename: &str, wav_bytes: &[u8], clip_id: u64) -> Result<PathBuf, String> {
    let ffmpeg =
        crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge()?;
    let output_path = save_file_dialog(default_filename, "MP3 Audio (*.mp3)", "*.mp3", "mp3")?;
    let temp_wav = std::env::temp_dir().join(format!("sgt_tts_playground_{clip_id}.wav"));
    std::fs::write(&temp_wav, wav_bytes).map_err(|err| err.to_string())?;

    let output = std::process::Command::new(&ffmpeg)
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
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOLDERID_Music, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FileOpenDialog,
        IFileOpenDialog, IShellItem, KNOWN_FOLDER_FLAG, SHCreateItemFromParsingName,
        SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|err| err.to_string())?;
        let _ = dialog.SetOptions(FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);
        let audio_name = wide("Audio files");
        let audio_pattern = wide("*.wav;*.flac;*.ogg;*.aiff;*.aif");
        let all_name = wide("All files");
        let all_pattern = wide("*.*");
        let file_types = [
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(audio_name.as_ptr()),
                pszSpec: PCWSTR(audio_pattern.as_ptr()),
            },
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(all_name.as_ptr()),
                pszSpec: PCWSTR(all_pattern.as_ptr()),
            },
        ];
        let _ = dialog.SetFileTypes(&file_types);
        if let Ok(music_path) = SHGetKnownFolderPath(&FOLDERID_Music, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(music_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }
        let result = dialog.GetResult().map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();
        Ok((!path_str.is_empty()).then(|| PathBuf::from(path_str)))
    }
}

#[cfg(windows)]
fn save_file_dialog_windows(
    default_name: &str,
    filter_name: &str,
    filter_pattern: &str,
    default_ext: &str,
) -> Result<PathBuf, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FOS_OVERWRITEPROMPT, FOS_STRICTFILETYPES, FileSaveDialog,
        IFileSaveDialog, IShellItem, KNOWN_FOLDER_FLAG, SHCreateItemFromParsingName,
        SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    let wide = |s: &str| -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL)
            .map_err(|err| format!("Create save dialog failed: {err}"))?;

        let filter_name = wide(filter_name);
        let filter_pattern = wide(filter_pattern);
        let file_types = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(filter_name.as_ptr()),
            pszSpec: PCWSTR(filter_pattern.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&file_types);
        let _ = dialog.SetFileTypeIndex(1);

        if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }

        let default_ext = wide(default_ext);
        let default_name = wide(default_name);
        let _ = dialog.SetDefaultExtension(PCWSTR(default_ext.as_ptr()));
        let _ = dialog.SetFileName(PCWSTR(default_name.as_ptr()));
        let _ = dialog.SetOptions(FOS_OVERWRITEPROMPT | FOS_STRICTFILETYPES);

        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Err("Save cancelled".to_string());
        }

        let result = dialog
            .GetResult()
            .map_err(|err| format!("Get save path failed: {err}"))?;
        let path = result
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|err| format!("Read save path failed: {err}"))?;
        let path_str = path.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();

        if path_str.is_empty() {
            Err("Save path is empty".to_string())
        } else {
            Ok(Path::new(&path_str).to_path_buf())
        }
    }
}
