use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use pollster::block_on;
use windows::Media::Capture::{
    MediaCapture, MediaCaptureInitializationSettings, MediaCaptureMemoryPreference,
    MediaCaptureSharingMode, StreamingCaptureMode,
};
use windows::Media::MediaProperties::{MediaEncodingProfile, VideoEncodingQuality};
use windows::Storage::StorageFile;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HSTRING;

const WEBCAM_POLL_SLEEP_MS: u64 = 20;

struct ComScope(bool);

impl ComScope {
    fn initialize_mta() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| format!("CoInitializeEx webcam thread: {e}"))?;
        Ok(Self(true))
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn build_profile(quality: VideoEncodingQuality) -> Result<MediaEncodingProfile, String> {
    let profile =
        MediaEncodingProfile::CreateMp4(quality).map_err(|e| format!("CreateMp4 profile: {e}"))?;
    let video = profile
        .Video()
        .map_err(|e| format!("Read webcam video profile: {e}"))?;

    match quality {
        VideoEncodingQuality::HD1080p => {
            let _ = video.SetWidth(1920);
            let _ = video.SetHeight(1080);
            let _ = video.SetBitrate(8_000_000);
        }
        VideoEncodingQuality::HD720p => {
            let _ = video.SetWidth(1280);
            let _ = video.SetHeight(720);
            let _ = video.SetBitrate(4_500_000);
        }
        _ => {}
    }

    if let Ok(frame_rate) = video.FrameRate() {
        let _ = frame_rate.SetNumerator(30);
        let _ = frame_rate.SetDenominator(1);
    }

    Ok(profile)
}

fn create_storage_file(path: &str) -> Result<StorageFile, String> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Create webcam directory {}: {e}", parent.display()))?;
    }

    let _ = std::fs::remove_file(path);
    std::fs::File::create(path).map_err(|e| format!("Create webcam file {path}: {e}"))?;

    let async_file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))
        .map_err(|e| format!("GetFileFromPathAsync: {e}"))?;
    block_on(async_file).map_err(|e| format!("Resolve webcam storage file: {e}"))
}

fn initialize_media_capture() -> Result<MediaCapture, String> {
    let settings = MediaCaptureInitializationSettings::new()
        .map_err(|e| format!("Create MediaCaptureInitializationSettings: {e}"))?;
    settings
        .SetStreamingCaptureMode(StreamingCaptureMode::Video)
        .map_err(|e| format!("SetStreamingCaptureMode(Video): {e}"))?;
    settings
        .SetSharingMode(MediaCaptureSharingMode::SharedReadOnly)
        .map_err(|e| format!("SetSharingMode(SharedReadOnly): {e}"))?;
    settings
        .SetMemoryPreference(MediaCaptureMemoryPreference::Auto)
        .map_err(|e| format!("SetMemoryPreference(Auto): {e}"))?;
    let _ = settings.SetAudioDeviceId(&HSTRING::new());

    let capture = MediaCapture::new().map_err(|e| format!("Create MediaCapture: {e}"))?;
    let init_action = capture
        .InitializeWithSettingsAsync(&settings)
        .map_err(|e| format!("InitializeWithSettingsAsync: {e}"))?;
    block_on(init_action).map_err(|e| format!("Initialize webcam capture: {e}"))?;
    Ok(capture)
}

fn start_recording(capture: &MediaCapture, file: &StorageFile) -> Result<(), String> {
    let qualities = [VideoEncodingQuality::HD1080p, VideoEncodingQuality::HD720p];
    let mut last_error = None;

    for quality in qualities {
        let profile = match build_profile(quality) {
            Ok(profile) => profile,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let action = match capture.StartRecordToStorageFileAsync(&profile, file) {
            Ok(action) => action,
            Err(error) => {
                last_error = Some(format!(
                    "StartRecordToStorageFileAsync({quality:?}): {error}"
                ));
                continue;
            }
        };
        match block_on(action) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(format!("Start webcam recording with {quality:?}: {error}"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "Failed to start webcam recording".to_string()))
}

pub(crate) fn record_webcam_video_sidecar(
    output_path: String,
    recording_start: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
    start_offset_ms: &'static AtomicU64,
) -> Result<(), String> {
    finished_signal.store(false, Ordering::SeqCst);
    start_offset_ms.store(u64::MAX, Ordering::SeqCst);

    thread::spawn(move || {
        let thread_result = (|| -> Result<(), String> {
            let _com_scope = ComScope::initialize_mta()?;
            let file = create_storage_file(&output_path)?;
            let capture = initialize_media_capture()?;
            start_recording(&capture, &file)?;
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            let _ = start_offset_ms.compare_exchange(
                u64::MAX,
                elapsed_ms,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );

            while !stop_signal.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(WEBCAM_POLL_SLEEP_MS));
            }

            let stop_action = capture
                .StopRecordAsync()
                .map_err(|e| format!("StopRecordAsync: {e}"))?;
            block_on(stop_action).map_err(|e| format!("Stop webcam recording: {e}"))?;
            let _ = capture.Close();

            Ok(())
        })();

        if let Err(error) = thread_result {
            eprintln!("[WebcamCapture] {}", error);
            let _ = std::fs::remove_file(&output_path);
        }

        if !Path::new(&output_path)
            .metadata()
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&output_path);
        }

        finished_signal.store(true, Ordering::SeqCst);
    });

    Ok(())
}
