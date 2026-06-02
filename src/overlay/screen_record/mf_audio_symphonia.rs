use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{Channels, Layout, SampleBuffer};
use symphonia::core::codecs::{CODEC_TYPE_NULL, CODEC_TYPE_OPUS, CodecRegistry, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};
use symphonia_adapter_libopus::OpusDecoder;

pub(super) struct SymphoniaAudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    source_sample_rate: u32,
    source_channels: u32,
    target_sample_rate: u32,
    target_channels: u32,
    next_pts_100ns: i64,
    pending_sample: Option<(Vec<u8>, i64)>,
}

impl SymphoniaAudioDecoder {
    pub(super) fn new(
        file_path: &str,
        target_sample_rate: Option<u32>,
        target_channels: Option<u32>,
    ) -> Result<Self, String> {
        let file = File::open(file_path).map_err(|e| format!("Open audio source: {e}"))?;
        let media_source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
        {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                media_source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("Symphonia probe: {e}"))?;
        let format = probed.format;
        let track = select_audio_track(format.tracks())
            .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

        let track_id = track.id;
        let mut codec_params = track.codec_params.clone();
        backfill_opus_codec_params(file_path, &mut codec_params);
        let time_base = codec_params.time_base;
        let next_pts_100ns = time_base
            .map(|time_base| timestamp_to_100ns(time_base, codec_params.start_ts))
            .unwrap_or(0);

        let mut codec_registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut codec_registry);
        codec_registry.register_all::<OpusDecoder>();
        let decoder = codec_registry
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Symphonia decoder init: {e}"))?;

        let mut decoder = Self {
            format,
            decoder,
            track_id,
            time_base,
            source_sample_rate: codec_params.sample_rate.unwrap_or(0),
            source_channels: codec_params
                .channels
                .map(|channels| channels.count() as u32)
                .unwrap_or(0),
            target_sample_rate: target_sample_rate.unwrap_or(codec_params.sample_rate.unwrap_or(0)),
            target_channels: target_channels.unwrap_or(
                codec_params
                    .channels
                    .map(|channels| channels.count() as u32)
                    .unwrap_or(0),
            ),
            next_pts_100ns,
            pending_sample: None,
        };

        let first_sample = decoder
            .decode_next_sample()
            .map_err(|error| format!("Symphonia first decode: {error}"))?
            .ok_or_else(|| "Symphonia: no decodable audio frames found".to_string())?;
        decoder.pending_sample = Some(first_sample);

        if decoder.source_sample_rate == 0 || decoder.target_sample_rate == 0 {
            return Err("Symphonia: missing sample rate after first decoded frame".to_string());
        }
        if decoder.source_channels == 0 || decoder.target_channels == 0 {
            return Err("Symphonia: missing channel layout after first decoded frame".to_string());
        }

        Ok(decoder)
    }

    pub(super) fn sample_rate(&self) -> u32 {
        self.target_sample_rate
    }

    pub(super) fn channels(&self) -> u32 {
        self.target_channels
    }

    pub(super) fn read_samples(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        if let Some(sample) = self.pending_sample.take() {
            return Ok(Some(sample));
        }

        self.decode_next_sample()
    }

    pub(super) fn seek(&mut self, position_100ns: i64) -> Result<(), String> {
        let position_100ns = position_100ns.max(0);
        let seek_time = Time::new(
            (position_100ns / 10_000_000) as u64,
            (position_100ns % 10_000_000) as f64 / 10_000_000.0,
        );
        let seeked_to = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: seek_time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| format!("Symphonia seek: {e}"))?;
        self.decoder.reset();
        self.pending_sample = None;
        self.next_pts_100ns = self
            .time_base
            .map(|time_base| timestamp_to_100ns(time_base, seeked_to.actual_ts))
            .unwrap_or(position_100ns);
        Ok(())
    }

    fn decode_next_sample(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(ref error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => continue,
                Err(error) => return Err(format!("Symphonia next_packet: {error}")),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let packet_timestamp_100ns = self
                .time_base
                .map(|time_base| timestamp_to_100ns(time_base, packet.ts()));

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

            let spec = *decoded.spec();
            if self.source_sample_rate == 0 {
                self.source_sample_rate = spec.rate;
            }
            if self.source_channels == 0 {
                self.source_channels = spec.channels.count() as u32;
            }
            if self.target_sample_rate == 0 {
                self.target_sample_rate = self.source_sample_rate;
            }
            if self.target_channels == 0 {
                self.target_channels = self.source_channels;
            }

            let mut sample_buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buffer.copy_interleaved_ref(decoded);

            let samples = sample_buffer.samples();
            if samples.is_empty() {
                continue;
            }

            let converted = convert_interleaved_f32(
                samples,
                self.source_channels.max(1) as usize,
                self.target_channels.max(1) as usize,
                self.source_sample_rate.max(1),
                self.target_sample_rate.max(1),
            );
            if converted.is_empty() {
                continue;
            }

            let timestamp_100ns = packet_timestamp_100ns.unwrap_or(self.next_pts_100ns);
            let samples_per_channel = converted.len() / (self.target_channels.max(1) as usize * 4);
            let duration_100ns =
                ((samples_per_channel as i64) * 10_000_000 / self.target_sample_rate as i64).max(1);
            self.next_pts_100ns = timestamp_100ns + duration_100ns;

            return Ok(Some((converted, timestamp_100ns)));
        }
    }
}

fn select_audio_track(
    tracks: &[symphonia::core::formats::Track],
) -> Option<&symphonia::core::formats::Track> {
    tracks.iter().find(|track| {
        let codec = track.codec_params.codec;
        codec != CODEC_TYPE_NULL
            && (track.codec_params.channels.is_some()
                || track.codec_params.channel_layout.is_some()
                || track.codec_params.sample_rate.is_some()
                || codec == CODEC_TYPE_OPUS)
    })
}

fn backfill_opus_codec_params(
    file_path: &str,
    codec_params: &mut symphonia::core::codecs::CodecParameters,
) {
    if codec_params.codec != CODEC_TYPE_OPUS {
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
        1 => Channels::FRONT_LEFT,
        2 => Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        3 => Channels::FRONT_LEFT | Channels::FRONT_RIGHT | Channels::FRONT_CENTRE,
        4 => {
            Channels::FRONT_LEFT
                | Channels::FRONT_RIGHT
                | Channels::REAR_LEFT
                | Channels::REAR_RIGHT
        }
        5 => {
            Channels::FRONT_LEFT
                | Channels::FRONT_RIGHT
                | Channels::FRONT_CENTRE
                | Channels::REAR_LEFT
                | Channels::REAR_RIGHT
        }
        _ => {
            Channels::FRONT_LEFT
                | Channels::FRONT_RIGHT
                | Channels::FRONT_CENTRE
                | Channels::LFE1
                | Channels::REAR_LEFT
                | Channels::REAR_RIGHT
        }
    };
    codec_params.with_channels(channels);

    match channel_count {
        1 => {
            codec_params.with_channel_layout(Layout::Mono);
        }
        2 => {
            codec_params.with_channel_layout(Layout::Stereo);
        }
        _ => {}
    }
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
    let search_end = bytes.len().saturating_sub(pattern.len() + 6);

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

fn convert_interleaved_f32(
    samples: &[f32],
    source_channels: usize,
    target_channels: usize,
    source_sample_rate: u32,
    target_sample_rate: u32,
) -> Vec<u8> {
    if source_channels == 0 || target_channels == 0 || samples.is_empty() {
        return Vec::new();
    }

    let remapped = remap_channels(samples, source_channels, target_channels);
    let resampled = if source_sample_rate != target_sample_rate {
        resample_interleaved_f32(
            &remapped,
            target_channels,
            source_sample_rate.max(1),
            target_sample_rate.max(1),
        )
    } else {
        remapped
    };

    bytemuck::cast_slice(&resampled).to_vec()
}

fn remap_channels(samples: &[f32], source_channels: usize, target_channels: usize) -> Vec<f32> {
    if source_channels == target_channels {
        return samples.to_vec();
    }

    let frame_count = samples.len() / source_channels;
    let mut remapped = Vec::with_capacity(frame_count * target_channels);
    for frame_index in 0..frame_count {
        let frame = &samples[frame_index * source_channels..(frame_index + 1) * source_channels];
        match (source_channels, target_channels) {
            (_, 1) => {
                let mono = frame.iter().copied().sum::<f32>() / frame.len() as f32;
                remapped.push(mono);
            }
            (1, dst) => {
                remapped.extend(std::iter::repeat_n(frame[0], dst));
            }
            (src, dst) if src >= dst => {
                remapped.extend_from_slice(&frame[..dst]);
            }
            _ => {
                remapped.extend_from_slice(frame);
                let fill = *frame.last().unwrap_or(&0.0);
                remapped.extend(std::iter::repeat_n(fill, target_channels - source_channels));
            }
        }
    }
    remapped
}

fn resample_interleaved_f32(
    samples: &[f32],
    channels: usize,
    source_sample_rate: u32,
    target_sample_rate: u32,
) -> Vec<f32> {
    if channels == 0 || samples.is_empty() || source_sample_rate == target_sample_rate {
        return samples.to_vec();
    }

    let source_frames = samples.len() / channels;
    if source_frames <= 1 {
        return samples.to_vec();
    }

    let target_frames = ((source_frames as u128) * (target_sample_rate as u128))
        .div_ceil(source_sample_rate as u128) as usize;
    let mut output = Vec::with_capacity(target_frames * channels);
    let ratio = source_sample_rate as f64 / target_sample_rate as f64;

    for target_frame in 0..target_frames {
        let source_pos = target_frame as f64 * ratio;
        let source_index = source_pos.floor() as usize;
        let next_index = (source_index + 1).min(source_frames - 1);
        let frac = (source_pos - source_index as f64) as f32;
        for channel in 0..channels {
            let start = samples[source_index * channels + channel];
            let end = samples[next_index * channels + channel];
            output.push(start + (end - start) * frac);
        }
    }

    output
}

fn timestamp_to_100ns(time_base: TimeBase, timestamp: u64) -> i64 {
    let time = time_base.calc_time(timestamp);
    (time.seconds as i64) * 10_000_000 + (time.frac * 10_000_000.0).round() as i64
}
