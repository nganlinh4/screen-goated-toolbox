use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::super::types::{
    AUDIO_PATH, MIC_AUDIO_PATH, MIC_AUDIO_START_OFFSET_MS, VIDEO_PATH, WEBCAM_VIDEO_PATH,
    WEBCAM_VIDEO_START_OFFSET_MS,
};

pub(super) struct RecordingPaths {
    pub(super) video_path: PathBuf,
    pub(super) mic_audio_path: PathBuf,
    pub(super) webcam_video_path: PathBuf,
}

pub(super) fn prepare_recording_paths() -> std::io::Result<RecordingPaths> {
    let app_data_dir = crate::paths::app_local_data_dir().join("recordings");

    std::fs::create_dir_all(&app_data_dir)?;

    let video_path = app_data_dir.join(format!(
        "recording_{}.mp4",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    let mic_audio_path =
        video_path.with_file_name(format!("{}_mic.wav", recording_stem(&video_path),));
    let webcam_video_path =
        video_path.with_file_name(format!("{}_webcam.mp4", recording_stem(&video_path),));

    Ok(RecordingPaths {
        video_path,
        mic_audio_path,
        webcam_video_path,
    })
}

pub(super) fn initialize_recording_paths(paths: &RecordingPaths) {
    *VIDEO_PATH.lock().unwrap() = Some(paths.video_path.to_string_lossy().to_string());
    *AUDIO_PATH.lock().unwrap() = Some(paths.video_path.to_string_lossy().to_string());
    *MIC_AUDIO_PATH.lock().unwrap() = None;
    *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
    MIC_AUDIO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
    WEBCAM_VIDEO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
}

fn recording_stem(video_path: &std::path::Path) -> &str {
    video_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("recording")
}
