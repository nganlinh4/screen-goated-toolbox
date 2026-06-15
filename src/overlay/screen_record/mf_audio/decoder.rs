use parking_lot::Mutex;

use super::super::mf_decode;
use super::mf_audio_symphonia::SymphoniaAudioDecoder;
use super::pcm::pcm_integer_bytes_to_f32le_bytes;
use windows::Win32::Media::MediaFoundation::*;

/// MF-based audio decoder that outputs PCM float samples.
pub struct MfAudioDecoder {
    sample_rate: u32,
    channels: u32,
    backend: AudioDecoderBackend,
}

enum AudioDecoderBackend {
    Mf(MfBackend),
    Symphonia(Mutex<SymphoniaAudioDecoder>),
}

struct MfBackend {
    reader: IMFSourceReader,
    audio_stream_index: u32,
    output_format: AudioOutputFormat,
}

#[derive(Clone, Copy, Debug)]
enum AudioOutputFormat {
    Float32,
    PcmInteger { bits_per_sample: u32 },
}

impl MfAudioDecoder {
    /// Open an audio file (or video file with audio track) for decoding.
    /// Outputs 32-bit float PCM at the source sample rate.
    pub fn new(file_path: &str) -> Result<Self, String> {
        Self::new_with_options(file_path, None, None, true)
    }

    /// Open an audio file with an optional caller-specified PCM float output format.
    pub fn new_with_output_format(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
    ) -> Result<Self, String> {
        Self::new_with_options(file_path, target_sample_rate, target_channels, false)
    }

    /// Open an audio file with a preferred output format but allow falling back
    /// to a native PCM shape if the exact transform is unavailable.
    pub fn new_with_preferred_output_format(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
    ) -> Result<Self, String> {
        Self::new_with_options(file_path, target_sample_rate, target_channels, true)
    }

    fn new_with_options(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
        allow_fallback_to_native_pcm: bool,
    ) -> Result<Self, String> {
        match Self::try_new_mf(
            file_path,
            target_sample_rate,
            target_channels,
            allow_fallback_to_native_pcm,
        ) {
            Ok(decoder) => Ok(decoder),
            Err(mf_error) => {
                let symphonia_decoder =
                    SymphoniaAudioDecoder::new(file_path, target_sample_rate, target_channels)
                        .map_err(|symphonia_error| {
                            format!("{mf_error}; Symphonia fallback: {symphonia_error}")
                        })?;
                println!(
                    "[MfAudioDecoder] Media Foundation unavailable, using Symphonia fallback: {}",
                    mf_error
                );
                Ok(Self {
                    sample_rate: symphonia_decoder.sample_rate(),
                    channels: symphonia_decoder.channels(),
                    backend: AudioDecoderBackend::Symphonia(Mutex::new(symphonia_decoder)),
                })
            }
        }
    }

    fn try_new_mf(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
        allow_fallback_to_native_pcm: bool,
    ) -> Result<Self, String> {
        mf_decode::mf_startup()?;

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

        negotiate_output_format(
            &reader,
            audio_idx,
            target_sample_rate,
            target_channels,
            allow_fallback_to_native_pcm,
        )?;

        let current_type = unsafe {
            reader
                .GetCurrentMediaType(audio_idx)
                .map_err(|e| format!("GetCurrentMediaType: {e}"))?
        };
        let output_format = detect_output_format(&current_type)?;
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
            "[MfAudioDecoder] Opened: {}Hz, {} channels, {:?}",
            sample_rate, channels, output_format
        );

        Ok(Self {
            sample_rate,
            channels,
            backend: AudioDecoderBackend::Mf(MfBackend {
                reader,
                audio_stream_index: audio_idx,
                output_format,
            }),
        })
    }

    /// Read the next chunk of PCM float audio.
    /// Returns `None` at end-of-stream.
    /// Returns `(pcm_data, timestamp_100ns)`.
    pub fn read_samples(&self) -> Result<Option<(Vec<u8>, i64)>, String> {
        match &self.backend {
            AudioDecoderBackend::Symphonia(decoder) => decoder.lock().read_samples(),
            AudioDecoderBackend::Mf(backend) => read_samples_mf(backend),
        }
    }

    /// Seek to a position in 100ns units.
    pub fn seek(&self, position_100ns: i64) -> Result<(), String> {
        match &self.backend {
            AudioDecoderBackend::Symphonia(decoder) => decoder.lock().seek(position_100ns),
            AudioDecoderBackend::Mf(backend) => seek_mf(backend, position_100ns),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }
}

fn read_samples_mf(backend: &MfBackend) -> Result<Option<(Vec<u8>, i64)>, String> {
    let mut stream_flags: u32 = 0;
    let mut timestamp: i64 = 0;
    let mut sample: Option<IMFSample> = None;

    unsafe {
        backend
            .reader
            .ReadSample(
                backend.audio_stream_index,
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

    let normalized = match backend.output_format {
        AudioOutputFormat::Float32 => data,
        AudioOutputFormat::PcmInteger { bits_per_sample } => {
            pcm_integer_bytes_to_f32le_bytes(&data, bits_per_sample)
        }
    };

    Ok(Some((normalized, timestamp)))
}

fn seek_mf(backend: &MfBackend, position_100ns: i64) -> Result<(), String> {
    let propvar = mf_decode::make_i64_propvariant(position_100ns);
    unsafe {
        backend
            .reader
            .SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar)
            .map_err(|e| format!("SetCurrentPosition audio: {e}"))?;
    }
    Ok(())
}

fn negotiate_output_format(
    reader: &IMFSourceReader,
    audio_idx: u32,
    target_sample_rate: Option<u32>,
    target_channels: Option<u32>,
    allow_fallback_to_native_pcm: bool,
) -> Result<(), String> {
    let mut errors = Vec::new();

    if target_sample_rate.is_some() || target_channels.is_some() {
        match try_set_output_type_exact(
            reader,
            audio_idx,
            &MFAudioFormat_Float,
            32,
            target_sample_rate,
            target_channels,
            "Float",
        ) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(error),
        }

        match try_set_output_type_exact(
            reader,
            audio_idx,
            &MFAudioFormat_PCM,
            16,
            target_sample_rate,
            target_channels,
            "PCM16",
        ) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(error),
        }
    }

    match try_set_output_type_partial(reader, audio_idx, &MFAudioFormat_PCM, "PCM(partial)") {
        Ok(()) => return Ok(()),
        Err(error) => errors.push(error),
    }

    if allow_fallback_to_native_pcm {
        match try_set_output_type_partial(reader, audio_idx, &MFAudioFormat_Float, "Float(partial)")
        {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(error),
        }
    }

    Err(errors.join("; "))
}

fn detect_output_format(current_type: &IMFMediaType) -> Result<AudioOutputFormat, String> {
    let subtype = unsafe {
        current_type
            .GetGUID(&MF_MT_SUBTYPE)
            .map_err(|e| format!("GetGUID SUBTYPE: {e}"))?
    };

    if subtype == MFAudioFormat_Float {
        return Ok(AudioOutputFormat::Float32);
    }

    if subtype == MFAudioFormat_PCM {
        let bits_per_sample = unsafe {
            current_type
                .GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)
                .unwrap_or(16)
        };
        return match bits_per_sample {
            8 | 16 | 24 | 32 => Ok(AudioOutputFormat::PcmInteger { bits_per_sample }),
            other => Err(format!("Unsupported PCM bits-per-sample: {other}")),
        };
    }

    Err(format!("Unsupported negotiated audio subtype: {subtype:?}"))
}

fn try_set_output_type_exact(
    reader: &IMFSourceReader,
    audio_idx: u32,
    subtype: &windows::core::GUID,
    bits_per_sample: u32,
    target_sample_rate: Option<u32>,
    target_channels: Option<u32>,
    label: &str,
) -> Result<(), String> {
    let pcm_type = unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
    unsafe {
        pcm_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;
        pcm_type
            .SetGUID(&MF_MT_SUBTYPE, subtype)
            .map_err(|e| format!("SetGUID SUBTYPE {label}: {e}"))?;
        pcm_type
            .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, bits_per_sample)
            .map_err(|e| format!("SetUINT32 BITS {label}: {e}"))?;
        if let Some(sample_rate) = target_sample_rate {
            pcm_type
                .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, sample_rate)
                .map_err(|e| format!("SetUINT32 SAMPLE_RATE {label}: {e}"))?;
        }
        if let Some(channels) = target_channels {
            pcm_type
                .SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)
                .map_err(|e| format!("SetUINT32 CHANNELS {label}: {e}"))?;
            let bytes_per_sample = bits_per_sample / 8;
            let block_align = channels * bytes_per_sample;
            pcm_type
                .SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align)
                .map_err(|e| format!("SetUINT32 BLOCK_ALIGN {label}: {e}"))?;
            if let Some(sample_rate) = target_sample_rate {
                pcm_type
                    .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, sample_rate * block_align)
                    .map_err(|e| format!("SetUINT32 AVG_BYTES {label}: {e}"))?;
            }
        }
        reader
            .SetCurrentMediaType(audio_idx, None, &pcm_type)
            .map_err(|e| format!("SetCurrentMediaType {label}: {e}"))?;
    }
    Ok(())
}

fn try_set_output_type_partial(
    reader: &IMFSourceReader,
    audio_idx: u32,
    subtype: &windows::core::GUID,
    label: &str,
) -> Result<(), String> {
    let partial_type =
        unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
    unsafe {
        partial_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(|e| format!("SetGUID MAJOR_TYPE: {e}"))?;
        partial_type
            .SetGUID(&MF_MT_SUBTYPE, subtype)
            .map_err(|e| format!("SetGUID SUBTYPE {label}: {e}"))?;
        reader
            .SetCurrentMediaType(audio_idx, None, &partial_type)
            .map_err(|e| format!("SetCurrentMediaType {label}: {e}"))?;
    }
    Ok(())
}
