use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{Channels, Position};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, CODEC_ID_NULL_AUDIO,
};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{TimeBase, Timestamp};
use symphonia_adapter_libopus::OpusDecoder;
use windows::Win32::Media::MediaFoundation::*;

use crate::overlay::screen_record::mf_audio::MfAudioDecoder;
use crate::overlay::screen_record::mf_decode;

use super::super::{NORMALIZED_IMPORT_AUDIO_CHANNELS, NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE};

pub(super) enum ImportAudioDecoder {
    Mf(MfAudioDecoder),
    Symphonia(SymphoniaImportAudioDecoder),
}

impl ImportAudioDecoder {
    pub(super) fn sample_rate(&self) -> u32 {
        match self {
            Self::Mf(decoder) => decoder.sample_rate(),
            Self::Symphonia(decoder) => decoder.sample_rate(),
        }
    }

    pub(super) fn channels(&self) -> u32 {
        match self {
            Self::Mf(decoder) => decoder.channels(),
            Self::Symphonia(decoder) => decoder.channels(),
        }
    }

    pub(super) fn read_samples(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        match self {
            Self::Mf(decoder) => decoder.read_samples(),
            Self::Symphonia(decoder) => decoder.read_samples(),
        }
    }
}

pub(super) struct SymphoniaImportAudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u32,
    time_base: Option<TimeBase>,
    next_pts_100ns: i64,
    pending_sample: Option<(Vec<u8>, i64)>,
}

impl SymphoniaImportAudioDecoder {
    fn new(file_path: &str) -> Result<Self, String> {
        let file = File::open(file_path).map_err(|e| format!("Open audio source: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
        {
            hint.with_extension(ext);
        }

        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("Symphonia probe: {e}"))?;

        let track = select_symphonia_audio_track(format.tracks())
            .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

        let track_id = track.id;
        let mut codec_params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .cloned()
            .ok_or_else(|| "Symphonia: selected track has no audio parameters".to_string())?;
        backfill_opus_codec_params(file_path, &mut codec_params);
        let time_base = track.time_base;
        let next_pts_100ns = time_base
            .map(|time_base| symphonia_timestamp_to_100ns(time_base, track.start_ts))
            .unwrap_or(0);

        let mut codec_registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut codec_registry);
        codec_registry.register_audio_decoder::<OpusDecoder>();
        let decoder = codec_registry
            .make_audio_decoder(&codec_params, &AudioDecoderOptions::default())
            .map_err(|e| format!("Symphonia decoder init: {e}"))?;

        let mut decoder = Self {
            format,
            decoder,
            track_id,
            sample_rate: codec_params.sample_rate.unwrap_or(0),
            channels: codec_params
                .channels
                .as_ref()
                .map(|channels| channels.count() as u32)
                .unwrap_or(0),
            time_base,
            next_pts_100ns,
            pending_sample: None,
        };

        let first_sample = decoder
            .decode_next_sample()
            .map_err(|error| format!("Symphonia first decode: {error}"))?
            .ok_or_else(|| "Symphonia: no decodable audio frames found".to_string())?;
        decoder.pending_sample = Some(first_sample);

        if decoder.sample_rate == 0 {
            return Err("Symphonia: missing sample rate after first decoded frame".to_string());
        }
        if decoder.channels == 0 {
            return Err("Symphonia: missing channel layout after first decoded frame".to_string());
        }

        Ok(decoder)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u32 {
        self.channels
    }

    fn read_samples(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        if let Some(sample) = self.pending_sample.take() {
            return Ok(Some(sample));
        }

        self.decode_next_sample()
    }

    fn decode_next_sample(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::IoError(ref error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => continue,
                Err(error) => return Err(format!("Symphonia next_packet: {error}")),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            let packet_timestamp_100ns = self
                .time_base
                .map(|time_base| symphonia_timestamp_to_100ns(time_base, packet.pts));

            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => continue,
                Err(SymphoniaError::IoError(ref error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(format!("Symphonia decode: {error}")),
            };

            let spec = decoded.spec();
            if self.sample_rate == 0 {
                self.sample_rate = spec.rate();
            }
            if self.channels == 0 {
                self.channels = spec.channels().count() as u32;
            }

            let mut samples = vec![0_f32; decoded.samples_interleaved()];
            decoded.copy_to_slice_interleaved(&mut samples);
            if samples.is_empty() {
                continue;
            }

            let timestamp_100ns = packet_timestamp_100ns.unwrap_or(self.next_pts_100ns);
            let channel_count = self.channels.max(1) as usize;
            let samples_per_channel = samples.len() / channel_count;
            let duration_100ns =
                ((samples_per_channel as i64) * 10_000_000 / self.sample_rate as i64).max(1);
            self.next_pts_100ns = timestamp_100ns + duration_100ns;

            return Ok(Some((
                bytemuck::cast_slice(&samples).to_vec(),
                timestamp_100ns,
            )));
        }
    }
}

pub(super) fn probe_media_has_audio(path: &Path) -> Result<bool, String> {
    probe_has_audio_track(&path.to_string_lossy())
}

/// Probe an audio file's duration in seconds without decoding any samples.
/// Uses symphonia which already covers every audio extension we accept
/// (mp3, wav, m4a, flac, ogg, aac, alac, aiff, opus, ...).
pub(super) fn probe_audio_duration_seconds(path: &Path) -> Result<f64, String> {
    let file = File::open(path).map_err(|e| format!("Open audio source: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("Symphonia probe: {e}"))?;

    let track = select_symphonia_audio_track(format.tracks())
        .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio());
    if let (Some(n_frames), Some(sr)) = (
        track.num_frames,
        params.and_then(|parameters| parameters.sample_rate),
    ) && sr > 0
    {
        return Ok(n_frames as f64 / sr as f64);
    }
    if let (Some(time_base), Some(duration)) = (track.time_base, track.duration)
        && let Some(time) = time_base.calc_duration(duration)
    {
        return Ok(time.as_secs_f64());
    }

    Err("Symphonia: cannot determine audio duration".to_string())
}

pub(super) fn open_import_audio_decoder(
    file_path: &str,
    trace_id: &str,
) -> Result<ImportAudioDecoder, String> {
    match MfAudioDecoder::new_with_preferred_output_format(
        file_path,
        Some(NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE),
        Some(NORMALIZED_IMPORT_AUDIO_CHANNELS),
    ) {
        Ok(decoder) => {
            let mode = if decoder.sample_rate() == NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE
                && decoder.channels() == NORMALIZED_IMPORT_AUDIO_CHANNELS
            {
                "normalized"
            } else {
                "native"
            };
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio] using {} decode {}Hz {}ch",
                trace_id,
                mode,
                decoder.sample_rate(),
                decoder.channels()
            );
            Ok(ImportAudioDecoder::Mf(decoder))
        }
        Err(error) => {
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio] Media Foundation does not support this audio track on this machine, trying Symphonia instead",
                trace_id
            );
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio][Detail] {}",
                trace_id,
                error
            );
            let decoder = SymphoniaImportAudioDecoder::new(file_path)?;
            crate::log_info!(
                "[VideoImport:{}][Normalize][Symphonia][Audio] using native decode {}Hz {}ch",
                trace_id,
                decoder.sample_rate(),
                decoder.channels()
            );
            Ok(ImportAudioDecoder::Symphonia(decoder))
        }
    }
}

fn symphonia_timestamp_to_100ns(time_base: TimeBase, timestamp: Timestamp) -> i64 {
    (time_base.calc_time_saturating(timestamp).as_nanos() / 100) as i64
}

fn select_symphonia_audio_track(
    tracks: &[symphonia::core::formats::Track],
) -> Option<&symphonia::core::formats::Track> {
    tracks
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .and_then(|params| params.audio())
                .is_some_and(|params| {
                    params.codec != CODEC_ID_NULL_AUDIO
                        && (params.sample_rate.is_some()
                            || params.channels.is_some()
                            || params.codec == CODEC_ID_OPUS)
                })
        })
        .or_else(|| {
            tracks.iter().find(|track| {
                track
                    .codec_params
                    .as_ref()
                    .and_then(|params| params.audio())
                    .is_some_and(|params| params.codec != CODEC_ID_NULL_AUDIO)
            })
        })
}

fn backfill_opus_codec_params(file_path: &str, codec_params: &mut AudioCodecParameters) {
    if codec_params.codec != CODEC_ID_OPUS {
        return;
    }

    if codec_params.sample_rate.is_none() {
        codec_params.with_sample_rate(48_000);
    }

    if codec_params.channels.is_some() {
        return;
    }

    let channel_count = opus_channel_count_from_extra_data(codec_params.extra_data.as_deref())
        .or_else(|| probe_opus_channel_count_from_mp4(file_path));

    let Some(channel_count) = channel_count else {
        return;
    };

    let channels = match channel_count {
        1 => Channels::Positioned(Position::FRONT_CENTER),
        2 => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT),
        count if count > 0 => Channels::Discrete(count.into()),
        _ => return,
    };
    codec_params.with_channels(channels);
}

fn opus_channel_count_from_extra_data(extra_data: Option<&[u8]>) -> Option<u8> {
    let extra_data = extra_data?;
    if extra_data.len() < 10 || &extra_data[..8] != b"OpusHead" {
        return None;
    }
    let count = extra_data[9];
    if count == 0 { None } else { Some(count) }
}

fn probe_opus_channel_count_from_mp4(file_path: &str) -> Option<u8> {
    let bytes = std::fs::read(file_path).ok()?;
    let pattern = b"dOps";
    // Need at least the 'dOps' tag + version byte + channel-count byte; the old
    // saturating_sub bottomed out at 0 and then sliced &bytes[0..4] on tiny files.
    if bytes.len() < pattern.len() + 6 {
        return None;
    }
    let search_end = bytes.len() - pattern.len();

    for index in 0..=search_end {
        if &bytes[index..index + pattern.len()] != pattern {
            continue;
        }

        let version = bytes.get(index + 4).copied().unwrap_or_default();
        let channel_count = bytes.get(index + 5).copied().unwrap_or_default();
        if version == 0 && channel_count > 0 {
            return Some(channel_count);
        }
    }

    None
}

pub(super) fn probe_has_audio_track(file_path: &str) -> Result<bool, String> {
    mf_decode::mf_startup()?;

    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();
    let reader = unsafe {
        MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
            .map_err(|e| format!("MFCreateSourceReaderFromURL audio probe: {e}"))?
    };

    let audio_index = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    Ok(unsafe { reader.GetNativeMediaType(audio_index, 0).is_ok() })
}
