use std::fs::{self, File};
use std::path::Path;
use std::slice;
use std::sync::atomic::{self, AtomicBool, AtomicUsize};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use parking_lot::{Condvar, Mutex};
use windows::core::{HSTRING, Interface};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Core::{
    AudioStreamDescriptor, MediaStreamSample, MediaStreamSource,
    MediaStreamSourceSampleRequestedEventArgs, MediaStreamSourceStartingEventArgs,
    VideoStreamDescriptor,
};
use windows::Media::MediaProperties::{
    AudioEncodingProperties, MediaEncodingProfile, MediaEncodingSubtypes,
    VideoEncodingProperties,
};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Streams::IRandomAccessStream;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

use super::{
    mf_hw_accel_enabled, saturating_decrement, video_frame_interval_100ns,
    AudioEncoderSource, AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder,
    VideoEncoderError, VideoEncoderSource, VideoSettingsBuilder,
};
use super::queue::create_video_stream_sample;

struct EncoderChannels {
    frame_sender: mpsc::Sender<Option<(VideoEncoderSource, TimeSpan)>>,
    audio_sender: mpsc::Sender<Option<(AudioEncoderSource, TimeSpan)>>,
    frame_notify: Arc<(Mutex<bool>, Condvar)>,
    audio_notify: Arc<(Mutex<bool>, Condvar)>,
    pending_video_frames: Arc<AtomicUsize>,
    pending_audio_buffers: Arc<AtomicUsize>,
    dropped_video_frames: Arc<AtomicUsize>,
    sample_requested: i64,
}

impl VideoEncoder {
    /// Creates a new `VideoEncoder` instance with the specified parameters.
    #[inline]
    pub fn new<P: AsRef<Path>>(
        video_settings: VideoSettingsBuilder,
        audio_settings: AudioSettingsBuilder,
        container_settings: ContainerSettingsBuilder,
        path: P,
    ) -> Result<Self, VideoEncoderError> {
        let stream = open_output_stream(path.as_ref())?;
        Self::new_from_stream(video_settings, audio_settings, container_settings, stream)
    }

    /// Creates a new `VideoEncoder` instance with the specified stream output.
    #[inline]
    pub fn new_from_stream(
        video_settings: VideoSettingsBuilder,
        audio_settings: AudioSettingsBuilder,
        container_settings: ContainerSettingsBuilder,
        stream: IRandomAccessStream,
    ) -> Result<Self, VideoEncoderError> {
        let media_encoding_profile = MediaEncodingProfile::new()?;

        let target_video_fps = video_settings.target_frame_rate().max(1);
        let video_frame_interval_100ns = video_frame_interval_100ns(target_video_fps);
        let (video_encoding_properties, is_video_disabled) = video_settings.build()?;
        media_encoding_profile.SetVideo(&video_encoding_properties)?;
        let (audio_encoding_properties, is_audio_disabled) = audio_settings.build()?;
        media_encoding_profile.SetAudio(&audio_encoding_properties)?;
        let container_encoding_properties = container_settings.build()?;
        media_encoding_profile.SetContainer(&container_encoding_properties)?;

        let media_stream_source =
            create_media_stream_source(&video_encoding_properties, &audio_encoding_properties)?;
        let starting = register_starting_handler(&media_stream_source)?;
        let channels =
            create_encoder_channels(&media_stream_source, is_video_disabled, is_audio_disabled)?;

        let error_notify = Arc::new(AtomicBool::new(false));
        let transcode_thread = spawn_transcode_thread(
            &media_stream_source,
            &stream,
            &media_encoding_profile,
            error_notify.clone(),
        )?;

        Ok(Self {
            first_timestamp: None,
            video_frame_interval_100ns,
            next_video_timestamp_100ns: 0,
            frame_sender: channels.frame_sender,
            audio_sender: channels.audio_sender,
            sample_requested: channels.sample_requested,
            media_stream_source,
            starting,
            transcode_thread: Some(transcode_thread),
            frame_notify: channels.frame_notify,
            audio_notify: channels.audio_notify,
            error_notify,
            pending_video_frames: channels.pending_video_frames,
            pending_audio_buffers: channels.pending_audio_buffers,
            dropped_video_frames: channels.dropped_video_frames,
            is_video_disabled,
            is_audio_disabled,
        })
    }
}

fn open_output_stream(path: &Path) -> Result<IRandomAccessStream, VideoEncoderError> {
    File::create(path)?;
    let canonical = fs::canonicalize(path)?;
    let canonical = canonical.to_string_lossy();
    let storage_path = canonical.strip_prefix(r"\\?\").unwrap_or(&canonical);
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(storage_path))?.join()?;
    Ok(file.OpenAsync(FileAccessMode::ReadWrite)?.join()?)
}

fn create_media_stream_source(
    video_encoding_properties: &VideoEncodingProperties,
    audio_encoding_properties: &AudioEncodingProperties,
) -> Result<MediaStreamSource, VideoEncoderError> {
    let video_encoding_properties = VideoEncodingProperties::CreateUncompressed(
        &MediaEncodingSubtypes::Bgra8()?,
        video_encoding_properties.Width()?,
        video_encoding_properties.Height()?,
    )?;
    let video_stream_descriptor = VideoStreamDescriptor::Create(&video_encoding_properties)?;

    let audio_encoding_properties = AudioEncodingProperties::CreatePcm(
        audio_encoding_properties.SampleRate()?,
        audio_encoding_properties.ChannelCount()?,
        16,
    )?;
    let audio_stream_descriptor = AudioStreamDescriptor::Create(&audio_encoding_properties)?;

    let media_stream_source =
        MediaStreamSource::CreateFromDescriptors(&video_stream_descriptor, &audio_stream_descriptor)?;
    media_stream_source.SetBufferTime(TimeSpan::default())?;

    Ok(media_stream_source)
}

fn register_starting_handler(media_stream_source: &MediaStreamSource) -> Result<i64, VideoEncoderError> {
    let token = media_stream_source.Starting(&TypedEventHandler::<
        MediaStreamSource,
        MediaStreamSourceStartingEventArgs,
    >::new(move |_, stream_start| {
        let stream_start = stream_start
            .as_ref()
            .expect("MediaStreamSource Starting parameter was None. This should not happen.");

        stream_start
            .Request()?
            .SetActualStartPosition(TimeSpan { Duration: 0 })?;
        Ok(())
    }))?;

    Ok(token)
}

fn create_encoder_channels(
    media_stream_source: &MediaStreamSource,
    is_video_disabled: bool,
    is_audio_disabled: bool,
) -> Result<EncoderChannels, VideoEncoderError> {
    let (frame_sender, frame_receiver) = mpsc::channel::<Option<(VideoEncoderSource, TimeSpan)>>();
    let (audio_sender, audio_receiver) = mpsc::channel::<Option<(AudioEncoderSource, TimeSpan)>>();

    let frame_notify = Arc::new((Mutex::new(false), Condvar::new()));
    let audio_notify = Arc::new((Mutex::new(false), Condvar::new()));
    let pending_video_frames = Arc::new(AtomicUsize::new(0));
    let pending_audio_buffers = Arc::new(AtomicUsize::new(0));
    let dropped_video_frames = Arc::new(AtomicUsize::new(0));

    let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<
        MediaStreamSource,
        MediaStreamSourceSampleRequestedEventArgs,
    >::new({
        let frame_notify = frame_notify.clone();
        let pending_video_frames = pending_video_frames.clone();
        let audio_notify = audio_notify.clone();
        let pending_audio_buffers = pending_audio_buffers.clone();

        move |_, sample_requested| {
            let sample_requested = sample_requested
                .as_ref()
                .expect("MediaStreamSource SampleRequested parameter was None. This should not happen.");

            if sample_requested
                .Request()?
                .StreamDescriptor()?
                .cast::<AudioStreamDescriptor>()
                .is_ok()
            {
                if is_audio_disabled {
                    sample_requested.Request()?.SetSample(None)?;
                    return Ok(());
                }

                let audio = match audio_receiver.recv() {
                    Ok(audio) => audio,
                    Err(error) => {
                        panic!("Failed to receive audio from the audio sender: {error}")
                    }
                };
                let has_audio_sample = audio.is_some();

                match audio {
                    Some((source, timestamp)) => {
                        let sample = create_audio_stream_sample(source, timestamp)?;
                        sample_requested.Request()?.SetSample(&sample)?;
                    }
                    None => {
                        sample_requested.Request()?.SetSample(None)?;
                    }
                }

                if has_audio_sample {
                    saturating_decrement(pending_audio_buffers.as_ref());
                }

                notify_consumer(&audio_notify);
            } else {
                if is_video_disabled {
                    sample_requested.Request()?.SetSample(None)?;
                    return Ok(());
                }

                let frame = match frame_receiver.recv() {
                    Ok(frame) => frame,
                    Err(error) => {
                        panic!("Failed to receive a frame from the frame sender: {error}")
                    }
                };
                let has_video_sample = frame.is_some();

                match frame {
                    Some((source, timestamp)) => {
                        let (sample, release_counter) =
                            create_video_stream_sample(source, timestamp)?;
                        if let Err(error) = sample_requested.Request()?.SetSample(&sample) {
                            if let Some(counter) = release_counter.as_ref() {
                                saturating_decrement(counter.as_ref());
                            }
                            return Err(error);
                        }
                        if let Some(counter) = release_counter.as_ref() {
                            saturating_decrement(counter.as_ref());
                        }
                    }
                    None => {
                        sample_requested.Request()?.SetSample(None)?;
                    }
                }

                if has_video_sample {
                    saturating_decrement(pending_video_frames.as_ref());
                }

                notify_consumer(&frame_notify);
            }

            Ok(())
        }
    }))?;

    Ok(EncoderChannels {
        frame_sender,
        audio_sender,
        frame_notify,
        audio_notify,
        pending_video_frames,
        pending_audio_buffers,
        dropped_video_frames,
        sample_requested,
    })
}

fn create_audio_stream_sample(
    source: AudioEncoderSource,
    timestamp: TimeSpan,
) -> Result<MediaStreamSample, windows::core::Error> {
    match source {
        AudioEncoderSource::Buffer(buffer_data) => {
            let buffer = buffer_data.0;
            let buffer = unsafe { slice::from_raw_parts(buffer.0, buffer_data.1) };
            let buffer = CryptographicBuffer::CreateFromByteArray(buffer)?;
            MediaStreamSample::CreateFromBuffer(&buffer, timestamp)
        }
        AudioEncoderSource::OwnedBuffer(buffer) => {
            let buffer = CryptographicBuffer::CreateFromByteArray(&buffer)?;
            MediaStreamSample::CreateFromBuffer(&buffer, timestamp)
        }
    }
}

fn notify_consumer(notify: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, cvar) = &**notify;
    *lock.lock() = true;
    cvar.notify_one();
}

fn spawn_transcode_thread(
    media_stream_source: &MediaStreamSource,
    stream: &IRandomAccessStream,
    media_encoding_profile: &MediaEncodingProfile,
    error_notify: Arc<AtomicBool>,
) -> Result<JoinHandle<Result<(), VideoEncoderError>>, VideoEncoderError> {
    let media_transcoder = MediaTranscoder::new()?;
    media_transcoder.SetHardwareAccelerationEnabled(mf_hw_accel_enabled())?;

    let transcode = media_transcoder
        .PrepareMediaStreamSourceTranscodeAsync(
            media_stream_source,
            stream,
            media_encoding_profile,
        )?
        .join()?;

    Ok(thread::spawn(move || -> Result<(), VideoEncoderError> {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }

        let result = transcode.TranscodeAsync();
        if result.is_err() {
            error_notify.store(true, atomic::Ordering::Relaxed);
        }

        result?.join()?;
        drop(media_transcoder);
        Ok(())
    }))
}
