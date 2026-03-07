use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use windows::Foundation::TimeSpan;
use windows::Graphics::DirectX::Direct3D11::IDirect3DSurface;
use windows::Media::Core::MediaStreamSample;

use crate::d3d11::SendDirectX;

use super::saturating_decrement;

#[derive(thiserror::Error, Debug)]
pub enum VideoEncoderError {
    #[error("Windows API error: {0}")]
    WindowsError(#[from] windows::core::Error),
    #[error("Failed to send frame: {0}")]
    FrameSendError(#[from] mpsc::SendError<Option<(VideoEncoderSource, TimeSpan)>>),
    #[error("Failed to send audio: {0}")]
    AudioSendError(#[from] mpsc::SendError<Option<(AudioEncoderSource, TimeSpan)>>),
    #[error("Video encoding is disabled")]
    VideoDisabled,
    #[error("CPU-backed video buffers are not supported by this zero-copy encoder path")]
    CpuVideoBufferUnsupported,
    #[error("Audio encoding is disabled")]
    AudioDisabled,
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

unsafe impl Send for VideoEncoderError {}
unsafe impl Sync for VideoEncoderError {}

/// The `VideoEncoderSource` enum represents all the types that can be sent to the encoder.
pub enum VideoEncoderSource {
    DirectX {
        surface: SendDirectX<IDirect3DSurface>,
        release_counter: Option<Arc<AtomicUsize>>,
    },
}

/// The `AudioEncoderSource` enum represents all the types that can be sent to the encoder.
pub enum AudioEncoderSource {
    Buffer((SendDirectX<*const u8>, usize)),
    OwnedBuffer(Vec<u8>),
}

#[derive(Clone)]
pub struct AudioEncoderHandle {
    sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>,
}

impl AudioEncoderHandle {
    #[inline]
    pub(super) fn new(sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>) -> Self {
        Self { sender }
    }

    #[inline]
    pub fn send_audio_buffer(
        &self,
        buffer: Vec<u8>,
        timestamp_100ns: i64,
    ) -> Result<(), VideoEncoderError> {
        self.sender.send(Some((
            AudioEncoderSource::OwnedBuffer(buffer),
            TimeSpan {
                Duration: timestamp_100ns,
            },
        )))?;
        Ok(())
    }
}

/// A handle that submits video frames at a constant framerate, independent of
/// the WGC frame delivery rate. Used for window capture where WGC only delivers
/// frames when the window content changes.
pub struct FramePump {
    frame_sender: mpsc::Sender<Option<(VideoEncoderSource, TimeSpan)>>,
    audio_sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>,
    pending_video_frames: Arc<AtomicUsize>,
    dropped_video_frames: Arc<AtomicUsize>,
    video_frame_interval_100ns: i64,
    next_video_timestamp_100ns: i64,
}

impl FramePump {
    #[inline]
    pub(super) fn new(
        frame_sender: mpsc::Sender<Option<(VideoEncoderSource, TimeSpan)>>,
        audio_sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>,
        pending_video_frames: Arc<AtomicUsize>,
        dropped_video_frames: Arc<AtomicUsize>,
        video_frame_interval_100ns: i64,
    ) -> Self {
        Self {
            frame_sender,
            audio_sender,
            pending_video_frames,
            dropped_video_frames,
            video_frame_interval_100ns,
            next_video_timestamp_100ns: 0,
        }
    }

    /// Submit a GPU surface as the next video frame.
    /// Returns `true` if the frame was queued, `false` if dropped due to backpressure.
    #[inline]
    pub fn submit_surface(
        &mut self,
        surface: SendDirectX<IDirect3DSurface>,
        max_pending: usize,
        release_counter: Option<Arc<AtomicUsize>>,
    ) -> bool {
        // Advance PTS clock unconditionally. Even dropped frames must consume
        // their slot in the timeline so video stays in sync with audio/mouse.
        let ts = TimeSpan {
            Duration: self.next_video_timestamp_100ns,
        };
        self.next_video_timestamp_100ns = self
            .next_video_timestamp_100ns
            .saturating_add(self.video_frame_interval_100ns);

        let max_pending = max_pending.max(1);
        if self.pending_video_frames.load(Ordering::Relaxed) >= max_pending {
            self.dropped_video_frames.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        if let Some(counter) = release_counter.as_ref() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        self.pending_video_frames.fetch_add(1, Ordering::Relaxed);
        let release_counter_for_send = release_counter.clone();
        if self
            .frame_sender
            .send(Some((
                VideoEncoderSource::DirectX {
                    surface,
                    release_counter: release_counter_for_send,
                },
                ts,
            )))
            .is_ok()
        {
            true
        } else {
            if let Some(counter) = release_counter.as_ref() {
                saturating_decrement(counter.as_ref());
            }
            saturating_decrement(self.pending_video_frames.as_ref());
            false
        }
    }

    /// Send EOF (None) to both video and audio channels, signaling the MF
    /// transcode that no more samples will arrive.
    pub fn signal_eof(&self) {
        let _ = self.frame_sender.send(None);
        let _ = self.audio_sender.send(None);
    }

    /// Number of frames waiting in the encoder queue.
    #[must_use]
    #[inline]
    pub fn pending_frames(&self) -> usize {
        self.pending_video_frames.load(Ordering::Relaxed)
    }

    /// Total frames dropped due to backpressure.
    #[must_use]
    #[inline]
    pub fn dropped_frames(&self) -> usize {
        self.dropped_video_frames.load(Ordering::Relaxed)
    }
}

unsafe impl Send for FramePump {}

#[inline]
pub(super) fn create_video_stream_sample(
    source: VideoEncoderSource,
    timestamp: TimeSpan,
) -> Result<(MediaStreamSample, Option<Arc<AtomicUsize>>), windows::core::Error> {
    match source {
        VideoEncoderSource::DirectX {
            surface,
            release_counter,
        } => {
            let sample = match MediaStreamSample::CreateFromDirect3D11Surface(&surface.0, timestamp)
            {
                Ok(sample) => sample,
                Err(error) => {
                    if let Some(counter) = release_counter.as_ref() {
                        saturating_decrement(counter.as_ref());
                    }
                    return Err(error);
                }
            };
            Ok((sample, release_counter))
        }
    }
}
