// Media Foundation hardware-accelerated video encoder.
// Encodes BGRA frames to H.264 MP4 via SinkWriter.

use windows::Win32::Media::MediaFoundation::*;

use super::mf_audio::{AudioConfig, AudioStream};
use super::mf_decode::DxgiDeviceManager;

/// Encoder codec selection.
#[derive(Debug, Clone, Copy)]
pub enum VideoCodec {
    H264,
}

/// Configuration for the MF encoder.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub bitrate_kbps: u32,
}

/// Media Foundation SinkWriter for hardware-accelerated video encode to MP4.
pub struct MfEncoder {
    writer: IMFSinkWriter,
    video_stream_index: u32,
    config: EncoderConfig,
}

impl MfEncoder {
    /// Create an encoder that writes to an MP4 file.
    ///
    /// The encoder accepts NV12 D3D11 textures as input and produces
    /// H.264 or HEVC encoded video in an MP4 container.
    pub fn new(
        output_path: &str,
        config: EncoderConfig,
        device_manager: &DxgiDeviceManager,
        audio_config: Option<&AudioConfig>,
    ) -> Result<(Self, Option<AudioStream>), String> {
        let attrs = create_writer_attributes(&device_manager.manager)?;

        let wide_path: Vec<u16> = output_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let writer = unsafe {
            MFCreateSinkWriterFromURL(
                windows::core::PCWSTR(wide_path.as_ptr()),
                None::<&IMFByteStream>,
                &attrs,
            )
            .map_err(|e| format!("MFCreateSinkWriterFromURL: {e}"))?
        };

        // Add output stream (encoded format)
        let output_type = create_output_media_type(&config)?;
        let video_stream_index = unsafe {
            writer
                .AddStream(&output_type)
                .map_err(|e| format!("AddStream: {e}"))?
        };

        // Set input format (NV12 from GPU)
        let input_type = create_input_media_type(&config)?;
        unsafe {
            writer
                .SetInputMediaType(video_stream_index, &input_type, None::<&IMFAttributes>)
                .map_err(|e| format!("SetInputMediaType: {e}"))?;
        }

        let audio_stream = if let Some(ac) = audio_config {
            Some(AudioStream::add_to_writer(&writer, ac)?)
        } else {
            None
        };

        unsafe {
            writer
                .BeginWriting()
                .map_err(|e| format!("BeginWriting: {e}"))?;
        }

        println!(
            "[MfEncoder] Created {:?} encoder {}x{} @ {}kbps, {}/{} fps (Has Audio: {}) → {}",
            config.codec,
            config.width,
            config.height,
            config.bitrate_kbps,
            config.fps_num,
            config.fps_den,
            audio_stream.is_some(),
            output_path
        );

        Ok((
            Self {
                writer,
                video_stream_index,
                config,
            },
            audio_stream,
        ))
    }

    pub fn writer(&self) -> &IMFSinkWriter {
        &self.writer
    }

    /// Write a CPU-resident BGRA frame as one encoded frame.
    ///
    /// Uses MFCreateMemoryBuffer to wrap CPU data. The SinkWriter internally
    /// uploads to GPU if hardware encode is available.
    pub fn write_frame_cpu(
        &self,
        bgra_data: &[u8],
        timestamp_100ns: i64,
        duration_100ns: i64,
    ) -> Result<(), String> {
        let expected_size = (self.config.width * self.config.height * 4) as usize;
        if bgra_data.len() != expected_size {
            return Err(format!(
                "BGRA data size mismatch: got {} expected {}",
                bgra_data.len(),
                expected_size
            ));
        }

        let buffer = unsafe {
            MFCreateMemoryBuffer(bgra_data.len() as u32)
                .map_err(|e| format!("MFCreateMemoryBuffer: {e}"))?
        };

        unsafe {
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            buffer
                .Lock(&mut data_ptr, None, None)
                .map_err(|e| format!("Lock: {e}"))?;
            std::ptr::copy_nonoverlapping(bgra_data.as_ptr(), data_ptr, bgra_data.len());
            buffer.Unlock().map_err(|e| format!("Unlock: {e}"))?;
            buffer
                .SetCurrentLength(bgra_data.len() as u32)
                .map_err(|e| format!("SetCurrentLength: {e}"))?;
        }

        let sample = unsafe { MFCreateSample().map_err(|e| format!("MFCreateSample: {e}"))? };

        unsafe {
            sample
                .AddBuffer(&buffer)
                .map_err(|e| format!("AddBuffer: {e}"))?;
            sample
                .SetSampleTime(timestamp_100ns)
                .map_err(|e| format!("SetSampleTime: {e}"))?;
            sample
                .SetSampleDuration(duration_100ns)
                .map_err(|e| format!("SetSampleDuration: {e}"))?;
        }

        unsafe {
            self.writer
                .WriteSample(self.video_stream_index, &sample)
                .map_err(|e| format!("WriteSample: {e}"))?;
        }

        Ok(())
    }

    /// Finalize the MP4 file (flush encoder, close container).
    pub fn finalize(self) -> Result<(), String> {
        unsafe {
            self.writer
                .Finalize()
                .map_err(|e| format!("Finalize: {e}"))?;
        }
        println!("[MfEncoder] Finalized MP4");
        Ok(())
    }

    /// Frame duration in 100ns units based on configured framerate.
    pub fn frame_duration_100ns(&self) -> i64 {
        (10_000_000i64 * self.config.fps_den as i64) / self.config.fps_num as i64
    }
}

/// Create IMFAttributes for the SinkWriter with D3D11 HW acceleration.
fn create_writer_attributes(manager: &IMFDXGIDeviceManager) -> Result<IMFAttributes, String> {
    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 3).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    unsafe {
        // Attach DXGI device manager for HW encode
        attrs
            .SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, manager)
            .map_err(|e| format!("SetUnknown SINK_D3D_MANAGER: {e}"))?;

        // Enable hardware transforms
        attrs
            .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
            .map_err(|e| format!("SetUINT32 HW_TRANSFORMS: {e}"))?;

        // Enable low-latency mode for faster encoding
        attrs
            .SetUINT32(&MF_LOW_LATENCY, 1)
            .map_err(|e| format!("SetUINT32 LOW_LATENCY: {e}"))?;
    }

    Ok(attrs)
}

/// Create the output (encoded) media type for the SinkWriter.
fn create_output_media_type(config: &EncoderConfig) -> Result<IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };

    let codec_guid = match config.codec {
        VideoCodec::H264 => MFVideoFormat_H264,
    };

    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;

        media_type
            .SetGUID(&MF_MT_SUBTYPE, &codec_guid)
            .map_err(|e| format!("SetGUID SUBTYPE: {e}"))?;

        // Bitrate in bits/sec
        media_type
            .SetUINT32(&MF_MT_AVG_BITRATE, config.bitrate_kbps * 1000)
            .map_err(|e| format!("SetUINT32 AVG_BITRATE: {e}"))?;

        // Frame size packed as (width << 32) | height
        let frame_size = ((config.width as u64) << 32) | (config.height as u64);
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .map_err(|e| format!("SetUINT64 FRAME_SIZE: {e}"))?;

        // Frame rate packed as (num << 32) | den
        let frame_rate = ((config.fps_num as u64) << 32) | (config.fps_den as u64);
        media_type
            .SetUINT64(&MF_MT_FRAME_RATE, frame_rate)
            .map_err(|e| format!("SetUINT64 FRAME_RATE: {e}"))?;

        // Interlace mode = progressive
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, 2) // MFVideoInterlace_Progressive = 2
            .map_err(|e| format!("SetUINT32 INTERLACE: {e}"))?;

        // H.264 High profile (or HEVC Main)
        let profile = match config.codec {
            VideoCodec::H264 => eAVEncH264VProfile_High.0 as u32,
        };
        media_type
            .SetUINT32(&MF_MT_MPEG2_PROFILE, profile)
            .map_err(|e| format!("SetUINT32 PROFILE: {e}"))?;
    }

    Ok(media_type)
}

/// Create the input (uncompressed NV12) media type for the SinkWriter.
fn create_input_media_type(config: &EncoderConfig) -> Result<IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };

    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;

        // ARGB32 = BGRA byte order on little-endian. The SinkWriter inserts
        // a color converter MFT to convert BGRA→NV12 before the HW encoder.
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)
            .map_err(|e| format!("SetGUID SUBTYPE ARGB32: {e}"))?;

        let frame_size = ((config.width as u64) << 32) | (config.height as u64);
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .map_err(|e| format!("SetUINT64 FRAME_SIZE: {e}"))?;

        let frame_rate = ((config.fps_num as u64) << 32) | (config.fps_den as u64);
        media_type
            .SetUINT64(&MF_MT_FRAME_RATE, frame_rate)
            .map_err(|e| format!("SetUINT64 FRAME_RATE: {e}"))?;

        // Progressive scan
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, 2)
            .map_err(|e| format!("SetUINT32 INTERLACE: {e}"))?;
    }

    Ok(media_type)
}
