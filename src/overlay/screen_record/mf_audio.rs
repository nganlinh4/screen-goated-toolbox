// Media Foundation audio decode, processing, and muxing.
// Decodes source audio to PCM, handles speed/trim, encodes to AAC.

use windows::Win32::Media::MediaFoundation::*;

/// Configuration for audio processing.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bitrate_kbps: 192,
        }
    }
}

/// MF-based audio decoder that outputs PCM float samples.
pub struct MfAudioDecoder {
    reader: IMFSourceReader,
    audio_stream_index: u32,
    sample_rate: u32,
    channels: u32,
}

/// Audio stream handle for the SinkWriter (shares the writer with video).
pub struct AudioStream {
    stream_index: u32,
}

impl MfAudioDecoder {
    /// Open an audio file (or video file with audio track) for decoding.
    /// Outputs 32-bit float PCM at the source sample rate.
    pub fn new(file_path: &str) -> Result<Self, String> {
        Self::new_with_output_format(file_path, None, None)
    }

    /// Open an audio file with an optional caller-specified PCM float output format.
    pub fn new_with_output_format(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
    ) -> Result<Self, String> {
        let mut attrs: Option<IMFAttributes> = None;
        unsafe {
            MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
        }
        let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

        // No D3D manager needed for audio-only decode
        unsafe {
            attrs
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(|e| format!("SetUINT32 HW_TRANSFORMS: {e}"))?;
        }

        let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();
        let reader = unsafe {
            MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };

        let audio_idx = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        let all_streams = MF_SOURCE_READER_ALL_STREAMS.0 as u32;

        // Deselect all, select audio only
        unsafe {
            reader
                .SetStreamSelection(all_streams, false)
                .map_err(|e| format!("Deselect all: {e}"))?;
            reader
                .SetStreamSelection(audio_idx, true)
                .map_err(|e| format!("Select audio: {e}"))?;
        }

        // Request PCM float output
        let pcm_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            pcm_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;
            pcm_type
                .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)
                .map_err(|e| format!("SetGUID SUBTYPE: {e}"))?;
            pcm_type
                .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)
                .map_err(|e| format!("SetUINT32 BITS: {e}"))?;
            if let Some(sample_rate) = target_sample_rate {
                pcm_type
                    .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, sample_rate)
                    .map_err(|e| format!("SetUINT32 SAMPLE_RATE: {e}"))?;
            }
            if let Some(channels) = target_channels {
                pcm_type
                    .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)
                    .map_err(|e| format!("SetUINT32 CHANNELS: {e}"))?;
                let block_align = channels * 4;
                pcm_type
                    .SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align)
                    .map_err(|e| format!("SetUINT32 BLOCK_ALIGN: {e}"))?;
                if let Some(sample_rate) = target_sample_rate {
                    pcm_type
                        .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, sample_rate * block_align)
                        .map_err(|e| format!("SetUINT32 AVG_BYTES: {e}"))?;
                }
            }
            reader
                .SetCurrentMediaType(audio_idx, None, &pcm_type)
                .map_err(|e| format!("SetCurrentMediaType Float: {e}"))?;
        }

        // Read back negotiated format
        let current_type = unsafe {
            reader
                .GetCurrentMediaType(audio_idx)
                .map_err(|e| format!("GetCurrentMediaType: {e}"))?
        };
        let sample_rate = unsafe {
            current_type
                .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .map_err(|e| format!("GetUINT32 SAMPLE_RATE: {e}"))?
        };
        let channels = unsafe {
            current_type
                .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                .map_err(|e| format!("GetUINT32 NUM_CHANNELS: {e}"))?
        };

        println!(
            "[MfAudioDecoder] Opened: {}Hz, {} channels",
            sample_rate, channels
        );

        Ok(Self {
            reader,
            audio_stream_index: audio_idx,
            sample_rate,
            channels,
        })
    }

    /// Read the next chunk of PCM float audio.
    /// Returns `None` at end-of-stream.
    /// Returns `(pcm_data, timestamp_100ns)`.
    pub fn read_samples(&self) -> Result<Option<(Vec<u8>, i64)>, String> {
        let mut stream_flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample: Option<IMFSample> = None;

        unsafe {
            self.reader
                .ReadSample(
                    self.audio_stream_index,
                    0,
                    None,
                    Some(&mut stream_flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|e| format!("ReadSample audio: {e}"))?;
        }

        if (stream_flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
            return Ok(None);
        }

        let sample = match sample {
            Some(s) => s,
            None => return Ok(None),
        };

        // Convert to contiguous buffer and copy out
        let buffer = unsafe {
            sample
                .ConvertToContiguousBuffer()
                .map_err(|e| format!("ConvertToContiguousBuffer: {e}"))?
        };

        let mut data_ptr: *mut u8 = std::ptr::null_mut();
        let mut length: u32 = 0;
        unsafe {
            buffer
                .Lock(&mut data_ptr, None, Some(&mut length))
                .map_err(|e| format!("Lock buffer: {e}"))?;
        }

        let data = unsafe { std::slice::from_raw_parts(data_ptr, length as usize).to_vec() };

        unsafe {
            let _ = buffer.Unlock();
        }

        Ok(Some((data, timestamp)))
    }

    /// Seek to a position in 100ns units.
    pub fn seek(&self, position_100ns: i64) -> Result<(), String> {
        let propvar = super::mf_decode::make_i64_propvariant(position_100ns);
        unsafe {
            self.reader
                .SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar)
                .map_err(|e| format!("SetCurrentPosition audio: {e}"))?;
        }
        Ok(())
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }
}

impl AudioStream {
    /// Add an AAC audio stream to an existing SinkWriter.
    /// Returns a handle for writing audio samples.
    pub fn add_to_writer(writer: &IMFSinkWriter, config: &AudioConfig) -> Result<Self, String> {
        // Output type: AAC
        let output_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            output_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;
            output_type
                .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)
                .map_err(|e| format!("SetGUID SUBTYPE AAC: {e}"))?;
            output_type
                .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                .map_err(|e| format!("SetUINT32 BITS: {e}"))?;
            output_type
                .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, config.sample_rate)
                .map_err(|e| format!("SetUINT32 SAMPLE_RATE: {e}"))?;
            output_type
                .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, config.channels)
                .map_err(|e| format!("SetUINT32 CHANNELS: {e}"))?;
            output_type
                .SetUINT32(
                    &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                    config.bitrate_kbps * 1000 / 8,
                )
                .map_err(|e| format!("SetUINT32 AVG_BYTES: {e}"))?;
        }

        let stream_index = unsafe {
            writer
                .AddStream(&output_type)
                .map_err(|e| format!("AddStream audio: {e}"))?
        };

        // Input type: PCM float
        let input_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            input_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;
            input_type
                .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)
                .map_err(|e| format!("SetGUID SUBTYPE Float: {e}"))?;
            input_type
                .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)
                .map_err(|e| format!("SetUINT32 BITS: {e}"))?;
            input_type
                .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, config.sample_rate)
                .map_err(|e| format!("SetUINT32 SAMPLE_RATE: {e}"))?;
            input_type
                .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, config.channels)
                .map_err(|e| format!("SetUINT32 CHANNELS: {e}"))?;

            let block_align = config.channels * 4; // 4 bytes per float sample
            input_type
                .SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align)
                .map_err(|e| format!("SetUINT32 BLOCK_ALIGN: {e}"))?;
            input_type
                .SetUINT32(
                    &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                    config.sample_rate * block_align,
                )
                .map_err(|e| format!("SetUINT32 AVG_BYTES_IN: {e}"))?;

            writer
                .SetInputMediaType(stream_index, &input_type, None::<&IMFAttributes>)
                .map_err(|e| format!("SetInputMediaType audio: {e}"))?;
        }

        println!(
            "[AudioStream] Added AAC stream idx={}, {}Hz {}ch @ {}kbps",
            stream_index, config.sample_rate, config.channels, config.bitrate_kbps
        );

        Ok(Self { stream_index })
    }

    /// Write raw PCM float audio data with explicit continuous timestamps.
    /// Used when interleaving audio/video samples in the native export pipeline.
    pub fn write_samples_direct(
        &self,
        writer: &IMFSinkWriter,
        pcm_data: &[u8],
        pts_100ns: i64,
        duration_100ns: i64,
    ) -> Result<(), String> {
        let sample = unsafe { MFCreateSample().map_err(|e| format!("MFCreateSample: {e}"))? };

        let buffer = unsafe {
            MFCreateMemoryBuffer(pcm_data.len() as u32)
                .map_err(|e| format!("MFCreateMemoryBuffer: {e}"))?
        };

        unsafe {
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            buffer
                .Lock(&mut data_ptr, None, None)
                .map_err(|e| format!("Lock: {e}"))?;
            std::ptr::copy_nonoverlapping(pcm_data.as_ptr(), data_ptr, pcm_data.len());
            buffer
                .SetCurrentLength(pcm_data.len() as u32)
                .map_err(|e| format!("SetCurrentLength: {e}"))?;
            buffer.Unlock().map_err(|e| format!("Unlock: {e}"))?;

            sample
                .AddBuffer(&buffer)
                .map_err(|e| format!("AddBuffer: {e}"))?;
            sample
                .SetSampleTime(pts_100ns)
                .map_err(|e| format!("SetSampleTime: {e}"))?;
            sample
                .SetSampleDuration(duration_100ns)
                .map_err(|e| format!("SetSampleDuration: {e}"))?;

            writer
                .WriteSample(self.stream_index, &sample)
                .map_err(|e| format!("WriteSample audio: {e}"))?;
        }

        Ok(())
    }
}
