use std::os::windows::process::CommandExt;
use std::path::Path;

/// Post-mux audio onto a video-only MP4 using FFmpeg (no re-encoding of video).
pub fn post_mux_audio(
    video_path: &Path,
    audio_path: &str,
    output_path: &Path,
    speed: f64,
    trim_start: f64,
    duration: f64,
    trim_segments: &[(f64, f64)],
) -> Result<(), String> {
    let ffmpeg_path = super::super::get_ffmpeg_path();
    if !ffmpeg_path.exists() {
        return Err("FFmpeg not found for audio mux".to_string());
    }

    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-y".to_string(),
        "-i".to_string(),
        video_path.to_str().unwrap().to_string(),
    ];

    // Audio input with trim
    if trim_segments.is_empty() {
        args.extend([
            "-ss".to_string(),
            trim_start.to_string(),
            "-t".to_string(),
            duration.to_string(),
        ]);
    }
    args.extend(["-i".to_string(), audio_path.to_string()]);

    // Audio filter for speed + trim segments
    let mut audio_filter = if !trim_segments.is_empty() {
        let select_expr: String = trim_segments
            .iter()
            .map(|(start, end)| format!("between(t,{:.6},{:.6})", start, end))
            .collect::<Vec<_>>()
            .join("+");
        format!("aselect='{}',asetpts=N/SR/TB", select_expr)
    } else {
        "anull".to_string()
    };
    if speed != 1.0 {
        audio_filter = format!("{},atempo={}", audio_filter, speed.clamp(0.5, 2.0));
    }

    args.extend([
        "-c:v".to_string(),
        "copy".to_string(),
        "-af".to_string(),
        audio_filter,
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        "-map".to_string(),
        "0:v".to_string(),
        "-map".to_string(),
        "1:a".to_string(),
        "-shortest".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        output_path.to_str().unwrap().to_string(),
    ]);

    println!("[Export][AudioMux] Running ffmpeg for audio post-mux");
    let output = std::process::Command::new(&ffmpeg_path)
        .args(&args)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| format!("FFmpeg audio mux spawn failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg audio mux failed: {}", stderr));
    }

    println!("[Export][AudioMux] Audio post-mux complete");
    Ok(())
}

pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FileOpenDialog, IFileOpenDialog, IShellItem,
        SHCreateItemFromParsingName, SHGetKnownFolderPath, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
        FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|e| e.to_string())?;

        let _ = dialog.SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);

        if let Some(dir) = initial_dir.filter(|d| !d.trim().is_empty()) {
            let dir_w: Vec<u16> = std::ffi::OsStr::new(&dir)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(dir_w.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
        } else if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
        {
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
        }

        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }

        let result = dialog.GetResult().map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();

        if path_str.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path_str))
        }
    }
}
