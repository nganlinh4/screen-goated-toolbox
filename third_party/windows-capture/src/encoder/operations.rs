use std::sync::atomic::{self, AtomicUsize, Ordering};
use std::sync::Arc;

use windows::Foundation::TimeSpan;
use windows::Graphics::DirectX::Direct3D11::IDirect3DSurface;

use crate::d3d11::SendDirectX;
use crate::frame::Frame;

use super::{
    saturating_decrement, AudioEncoderHandle, AudioEncoderSource, FramePump, VideoEncoder,
    VideoEncoderError, VideoEncoderSource,
};

impl VideoEncoder {
    /// Sends a video frame to the video encoder for encoding.
    #[inline]
    pub fn send_frame(&mut self, frame: &mut Frame) -> Result<(), VideoEncoderError> {
        if self.is_video_disabled {
            return Err(VideoEncoderError::VideoDisabled);
        }

        let timestamp = self.next_video_timespan();

        self.pending_video_frames.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.frame_sender.send(Some((
            VideoEncoderSource::DirectX {
                surface: SendDirectX::new(unsafe { frame.as_raw_surface().clone() }),
                release_counter: None,
            },
            timestamp,
        ))) {
            saturating_decrement(self.pending_video_frames.as_ref());
            return Err(error.into());
        }

        wait_for_notify(&self.frame_notify);
        self.join_transcode_if_error()
    }

    /// Sends a video frame without blocking on encoder consumption.
    ///
    /// Returns `Ok(true)` when the frame is queued and `Ok(false)` when dropped because
    /// the pending queue is already at `max_pending_frames`.
    #[inline]
    pub fn send_frame_nonblocking(
        &mut self,
        frame: &mut Frame,
        max_pending_frames: usize,
    ) -> Result<bool, VideoEncoderError> {
        if self.is_video_disabled {
            return Err(VideoEncoderError::VideoDisabled);
        }

        let max_pending_frames = max_pending_frames.max(1);
        if self.pending_video_frames.load(Ordering::Relaxed) >= max_pending_frames {
            self.dropped_video_frames.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }

        let timestamp = self.next_video_timespan();

        self.pending_video_frames.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.frame_sender.send(Some((
            VideoEncoderSource::DirectX {
                surface: SendDirectX::new(unsafe { frame.as_raw_surface().clone() }),
                release_counter: None,
            },
            timestamp,
        ))) {
            saturating_decrement(self.pending_video_frames.as_ref());
            return Err(error.into());
        }

        self.join_transcode_if_error()?;
        Ok(true)
    }

    /// Sends a WinRT Direct3D surface without blocking on encoder consumption.
    ///
    /// Returns `Ok(true)` when the frame is queued and `Ok(false)` when dropped because
    /// the pending queue is already at `max_pending_frames`.
    #[inline]
    pub fn send_directx_surface_nonblocking(
        &mut self,
        surface: SendDirectX<IDirect3DSurface>,
        max_pending_frames: usize,
        release_counter: Option<Arc<AtomicUsize>>,
    ) -> Result<bool, VideoEncoderError> {
        if self.is_video_disabled {
            return Err(VideoEncoderError::VideoDisabled);
        }

        let max_pending_frames = max_pending_frames.max(1);
        if self.pending_video_frames.load(Ordering::Relaxed) >= max_pending_frames {
            self.dropped_video_frames.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }

        let timestamp = self.next_video_timespan();

        if let Some(counter) = release_counter.as_ref() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        self.pending_video_frames.fetch_add(1, Ordering::Relaxed);
        let release_counter_for_send = release_counter.clone();
        if let Err(error) = self.frame_sender.send(Some((
            VideoEncoderSource::DirectX {
                surface,
                release_counter: release_counter_for_send,
            },
            timestamp,
        ))) {
            if let Some(counter) = release_counter.as_ref() {
                saturating_decrement(counter.as_ref());
            }
            saturating_decrement(self.pending_video_frames.as_ref());
            return Err(error.into());
        }

        self.join_transcode_if_error()?;
        Ok(true)
    }

    /// Sends an owned BGRA buffer without blocking on encoder consumption.
    ///
    /// The buffer length must match the encoder's configured frame byte size.
    #[inline]
    pub fn send_frame_owned_buffer_nonblocking(
        &mut self,
        _buffer: Vec<u8>,
        _max_pending_frames: usize,
    ) -> Result<bool, VideoEncoderError> {
        Err(VideoEncoderError::CpuVideoBufferUnsupported)
    }

    #[must_use]
    #[inline]
    pub fn pending_video_frames(&self) -> usize {
        self.pending_video_frames.load(Ordering::Relaxed)
    }

    #[must_use]
    #[inline]
    pub fn dropped_video_frames(&self) -> usize {
        self.dropped_video_frames.load(Ordering::Relaxed)
    }

    #[must_use]
    #[inline]
    pub fn create_audio_handle(&self) -> AudioEncoderHandle {
        AudioEncoderHandle::new(self.audio_sender.clone())
    }

    /// Creates a `FramePump` that can submit video frames at a constant
    /// framerate from a background thread. When using a FramePump the
    /// caller must NOT call `send_frame*` or `skip_video_frames` on this
    /// encoder. The pump owns the video timeline.
    #[must_use]
    #[inline]
    pub fn create_frame_pump(&self) -> FramePump {
        FramePump::new(
            self.frame_sender.clone(),
            self.audio_sender.clone(),
            self.pending_video_frames.clone(),
            self.dropped_video_frames.clone(),
            self.video_frame_interval_100ns,
        )
    }

    /// Advances the internal video timeline without queueing a frame.
    ///
    /// This is used when the caller intentionally skips one or more output ticks
    /// (for example, due to backpressure) and still needs monotonic CFR timing.
    #[inline]
    pub fn skip_video_frames(&mut self, count: u32) {
        if count == 0 || self.is_video_disabled {
            return;
        }

        self.next_video_timestamp_100ns = self
            .next_video_timestamp_100ns
            .saturating_add(self.video_frame_interval_100ns.saturating_mul(count as i64));
    }

    /// Sends a video frame with audio to the video encoder for encoding.
    #[inline]
    pub fn send_frame_with_audio(
        &mut self,
        frame: &mut Frame,
        audio_buffer: &[u8],
    ) -> Result<(), VideoEncoderError> {
        if self.is_video_disabled {
            return Err(VideoEncoderError::VideoDisabled);
        }

        if self.is_audio_disabled {
            return Err(VideoEncoderError::AudioDisabled);
        }

        let timestamp = self.next_video_timespan();

        self.pending_video_frames.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.frame_sender.send(Some((
            VideoEncoderSource::DirectX {
                surface: SendDirectX::new(unsafe { frame.as_raw_surface().clone() }),
                release_counter: None,
            },
            timestamp,
        ))) {
            saturating_decrement(self.pending_video_frames.as_ref());
            return Err(error.into());
        }

        wait_for_notify(&self.frame_notify);
        self.join_transcode_if_error()?;

        self.pending_audio_buffers.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.audio_sender.send(Some((
            AudioEncoderSource::Buffer((SendDirectX::new(audio_buffer.as_ptr()), audio_buffer.len())),
            timestamp,
        ))) {
            saturating_decrement(self.pending_audio_buffers.as_ref());
            return Err(error.into());
        }

        wait_for_notify(&self.audio_notify);
        self.join_transcode_if_error()
    }

    /// Sends a video frame to the video encoder for encoding.
    #[inline]
    pub fn send_frame_buffer(
        &mut self,
        _buffer: &[u8],
        _timestamp: i64,
    ) -> Result<(), VideoEncoderError> {
        Err(VideoEncoderError::CpuVideoBufferUnsupported)
    }

    /// Sends audio to the video encoder for encoding.
    #[inline]
    pub fn send_audio_buffer(
        &mut self,
        buffer: &[u8],
        timestamp: i64,
    ) -> Result<(), VideoEncoderError> {
        if self.is_audio_disabled {
            return Err(VideoEncoderError::AudioDisabled);
        }

        let audio_timestamp = timestamp;
        let timestamp = match self.first_timestamp {
            Some(timestamp) => TimeSpan {
                Duration: audio_timestamp - timestamp.Duration,
            },
            None => {
                self.first_timestamp = Some(TimeSpan {
                    Duration: audio_timestamp,
                });
                TimeSpan { Duration: 0 }
            }
        };

        self.pending_audio_buffers.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.audio_sender.send(Some((
            AudioEncoderSource::Buffer((SendDirectX::new(buffer.as_ptr()), buffer.len())),
            timestamp,
        ))) {
            saturating_decrement(self.pending_audio_buffers.as_ref());
            return Err(error.into());
        }

        wait_for_notify(&self.audio_notify);
        self.join_transcode_if_error()
    }

    /// Finishes encoding the video and performs any necessary cleanup.
    #[inline]
    pub fn finish(mut self) -> Result<(), VideoEncoderError> {
        self.frame_sender.send(None)?;
        self.audio_sender.send(None)?;

        if let Some(transcode_thread) = self.transcode_thread.take() {
            transcode_thread
                .join()
                .expect("Failed to join transcode thread")?;
        }

        self.media_stream_source.RemoveStarting(self.starting)?;
        self.media_stream_source
            .RemoveSampleRequested(self.sample_requested)?;

        Ok(())
    }

    #[inline]
    fn next_video_timespan(&mut self) -> TimeSpan {
        let timestamp = TimeSpan {
            Duration: self.next_video_timestamp_100ns,
        };
        self.next_video_timestamp_100ns = self
            .next_video_timestamp_100ns
            .saturating_add(self.video_frame_interval_100ns);
        timestamp
    }

    #[inline]
    fn join_transcode_if_error(&mut self) -> Result<(), VideoEncoderError> {
        if self.error_notify.load(atomic::Ordering::Relaxed) {
            if let Some(transcode_thread) = self.transcode_thread.take() {
                transcode_thread
                    .join()
                    .expect("Failed to join transcode thread")?;
            }
        }

        Ok(())
    }
}

impl Drop for VideoEncoder {
    #[inline]
    fn drop(&mut self) {
        let _ = self.frame_sender.send(None);
        let _ = self.audio_sender.send(None);

        if let Some(transcode_thread) = self.transcode_thread.take() {
            let _ = transcode_thread.join();
        }
    }
}

fn wait_for_notify(notify: &Arc<(parking_lot::Mutex<bool>, parking_lot::Condvar)>) {
    let (lock, cvar) = &**notify;
    let mut processed = lock.lock();
    if !*processed {
        cvar.wait(&mut processed);
    }
    *processed = false;
}
