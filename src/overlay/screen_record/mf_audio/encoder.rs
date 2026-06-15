use super::AudioConfig;
use windows::Win32::Media::MediaFoundation::*;

/// Audio stream handle for the SinkWriter (shares the writer with video).
pub struct AudioStream {
    stream_index: u32,
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
