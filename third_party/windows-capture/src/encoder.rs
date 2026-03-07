use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use parking_lot::{Condvar, Mutex};
use windows::Foundation::TimeSpan;
use windows::Media::Core::MediaStreamSource;

mod construct;
mod image;
mod operations;
mod queue;
mod settings;

pub use self::image::{ImageEncoder, ImageEncoderError};
pub use self::queue::{
    AudioEncoderHandle, AudioEncoderSource, FramePump, VideoEncoderError, VideoEncoderSource,
};
pub use self::settings::{
    AudioSettingsBuilder, AudioSettingsSubType, ContainerSettingsBuilder,
    ContainerSettingsSubType, VideoSettingsBuilder, VideoSettingsSubType,
};

/// The `VideoEncoder` struct is used to encode video frames and save them to a specified file path.
pub struct VideoEncoder {
    first_timestamp: Option<TimeSpan>,
    video_frame_interval_100ns: i64,
    next_video_timestamp_100ns: i64,
    frame_sender: mpsc::Sender<Option<(VideoEncoderSource, TimeSpan)>>,
    audio_sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>,
    sample_requested: i64,
    media_stream_source: MediaStreamSource,
    starting: i64,
    transcode_thread: Option<JoinHandle<Result<(), VideoEncoderError>>>,
    frame_notify: Arc<(Mutex<bool>, Condvar)>,
    audio_notify: Arc<(Mutex<bool>, Condvar)>,
    error_notify: Arc<AtomicBool>,
    pending_video_frames: Arc<AtomicUsize>,
    pending_audio_buffers: Arc<AtomicUsize>,
    dropped_video_frames: Arc<AtomicUsize>,
    is_video_disabled: bool,
    is_audio_disabled: bool,
}

#[inline]
pub(super) fn saturating_decrement(counter: &AtomicUsize) {
    let previous = counter.fetch_sub(1, Ordering::Relaxed);
    if previous == 0 {
        counter.store(0, Ordering::Relaxed);
    }
}

#[inline]
pub(super) fn video_frame_interval_100ns(frame_rate: u32) -> i64 {
    let fps = frame_rate.max(1) as i64;
    // Use integer-rounded 100ns frame duration for stable CFR timeline.
    (10_000_000 + (fps / 2)) / fps
}

#[inline]
pub(super) fn mf_hw_accel_enabled() -> bool {
    match std::env::var("SCREEN_RECORD_MF_HW_ACCEL") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        // Default to software-transcode mode for stability under heavy GPU contention.
        Err(_) => false,
    }
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for VideoEncoder {}
