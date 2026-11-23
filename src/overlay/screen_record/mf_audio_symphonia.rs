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
#[cfg(feature = "recorder-worker")]
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
#[cfg(feature = "recorder-worker")]
use symphonia::core::units::Time;
use symphonia::core::units::TimeBase;
use symphonia_adapter_libopus::OpusDecoder;

use super::symphonia_convert::{LinearResampler, channel_positions, remap_channels};

pub(super) struct SymphoniaAudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    source_sample_rate: u32,
    source_channels: u32,
    target_sample_rate: u32,
    target_channels: u32,
    next_pts_100ns: i64,
    pending_sample: Option<(Vec<u8>, i64)>,
    source_positions: Option<Vec<Position>>,
    resampler: Option<LinearResampler>,
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

        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                media_source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("Symphonia probe: {e}"))?;
        let track = select_audio_track(format.tracks())
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
            .map(|time_base| timestamp_to_100ns(time_base, track.start_ts))
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
            time_base,
            source_sample_rate: codec_params.sample_rate.unwrap_or(0),
            source_channels: codec_params
                .channels
                .as_ref()
                .map(|channels| channels.count() as u32)
                .unwrap_or(0),
            target_sample_rate: target_sample_rate.unwrap_or(codec_params.sample_rate.unwrap_or(0)),
            target_channels: target_channels.unwrap_or(
                codec_params
                    .channels
                    .as_ref()
                    .map(|channels| channels.count() as u32)
                    .unwrap_or(0),
            ),
            next_pts_100ns,
            pending_sample: None,
            source_positions: None,
            resampler: None,
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

    #[cfg(feature = "recorder-worker")]
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

    #[cfg(feature = "recorder-worker")]
    pub(super) fn seek(&mut self, position_100ns: i64) -> Result<(), String> {
        let position_100ns = position_100ns.max(0);
        let seek_time = Time::try_new(
            position_100ns / 10_000_000,
            ((position_100ns % 10_000_000) * 100) as u32,
        )
        .ok_or_else(|| "Symphonia seek position is out of range".to_string())?;
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
        if let Some(resampler) = &mut self.resampler {
            resampler.reset();
        }
        self.pending_sample = None;
        self.next_pts_100ns = self
            .time_base
            .map(|time_base| timestamp_to_100ns(time_base, seeked_to.actual_ts))
            .unwrap_or(position_100ns);
        Ok(())
    }

    /// Remap channels by speaker position, resample with state carried across
    /// packets, and encode as little-endian f32 bytes.
    fn convert(&mut self, samples: &[f32]) -> Vec<u8> {
        let source_channels = self.source_channels.max(1) as usize;
        let target_channels = self.target_channels.max(1) as usize;
        let (source_rate, target_rate) = (
            self.source_sample_rate.max(1),
            self.target_sample_rate.max(1),
        );
        let remapped = remap_channels(
            samples,
            self.source_positions.as_deref(),
            source_channels,
            target_channels,
        );
        let resampled = if source_rate == target_rate {
            remapped
        } else {
            if self.resampler.as_ref().is_some_and(|resampler| {
                !resampler.matches(target_channels, source_rate, target_rate)
            }) {
                self.resampler = None;
            }
            self.resampler
                .get_or_insert_with(|| {
                    LinearResampler::new(target_channels, source_rate, target_rate)
                })
                .process(&remapped)
        };
        resampled.into_iter().flat_map(f32::to_le_bytes).collect()
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
                .map(|time_base| timestamp_to_100ns(time_base, packet.pts));

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
            if self.source_sample_rate == 0 {
                self.source_sample_rate = spec.rate();
            }
            if self.source_channels == 0 {
                self.source_channels = spec.channels().count() as u32;
            }
            if self.target_sample_rate == 0 {
                self.target_sample_rate = self.source_sample_rate;
            }
            if self.target_channels == 0 {
                self.target_channels = self.source_channels;
            }

            let mut samples = vec![0_f32; decoded.samples_interleaved()];
            decoded.copy_to_slice_interleaved(&mut samples);
            if samples.is_empty() {
                continue;
            }

            if self.source_positions.is_none() {
                self.source_positions = channel_positions(spec.channels());
            }

            let converted = self.convert(&samples);
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
        let Some(params) = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
        else {
            return false;
        };
        params.codec != CODEC_ID_NULL_AUDIO
            && (params.channels.is_some()
                || params.sample_rate.is_some()
                || params.codec == CODEC_ID_OPUS)
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
        1 => Position::FRONT_CENTER,
        2 => Position::FRONT_LEFT | Position::FRONT_RIGHT,
        3 => Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER,
        4 => {
            Position::FRONT_LEFT
                | Position::FRONT_RIGHT
                | Position::REAR_LEFT
                | Position::REAR_RIGHT
        }
        5 => {
            Position::FRONT_LEFT
                | Position::FRONT_RIGHT
                | Position::FRONT_CENTER
                | Position::REAR_LEFT
                | Position::REAR_RIGHT
        }
        _ => {
            Position::FRONT_LEFT
                | Position::FRONT_RIGHT
                | Position::FRONT_CENTER
                | Position::LFE1
                | Position::REAR_LEFT
                | Position::REAR_RIGHT
        }
    };
    codec_params.with_channels(Channels::Positioned(channels));
}

fn opus_channel_count_from_extra_data(extra_data: Option<&[u8]>) -> Option<u8> {
    let extra_data = extra_data?;
    if extra_data.len() < 10 || &extra_data[..8] != b"OpusHead" {
        return None;
    }
    let count = extra_data[9];
    if count == 0 { None } else { Some(count) }
}

/// Reads the Opus output channel count from an MP4 `dOps` box by walking the
/// box tree (moov/trak/mdia/minf/stbl/stsd/Opus/dOps) with seeks, so only box
/// headers are read whatever the size of the media payload.
fn probe_opus_channel_count_from_mp4(file_path: &str) -> Option<u8> {
    let mut file = File::open(file_path).ok()?;
    let end = file.metadata().ok()?.len();
    find_dops_channel_count(&mut file, 0, end, 0)
}

const MAX_BOX_DEPTH: u32 = 8;
const MAX_BOXES_PER_LEVEL: u32 = 4096;
/// AudioSampleEntry fields that precede an `Opus` entry's child boxes.
const AUDIO_SAMPLE_ENTRY_BYTES: u64 = 28;
/// `stsd` full-box header (version/flags) plus entry count.
const STSD_HEADER_BYTES: u64 = 8;

fn find_dops_channel_count(
    reader: &mut (impl std::io::Read + std::io::Seek),
    start: u64,
    end: u64,
    depth: u32,
) -> Option<u8> {
    use std::io::SeekFrom;
    if depth > MAX_BOX_DEPTH {
        return None;
    }
    let mut offset = start;
    for _ in 0..MAX_BOXES_PER_LEVEL {
        if offset.checked_add(8)? > end {
            return None;
        }
        reader.seek(SeekFrom::Start(offset)).ok()?;
        let mut header = [0u8; 8];
        reader.read_exact(&mut header).ok()?;
        let declared = u32::from_be_bytes(header[..4].try_into().ok()?) as u64;
        let kind = &header[4..8];
        let (box_size, header_len) = match declared {
            0 => (end - offset, 8),
            1 => {
                let mut large = [0u8; 8];
                reader.read_exact(&mut large).ok()?;
                (u64::from_be_bytes(large), 16)
            }
            size => (size, 8),
        };
        if box_size < header_len {
            return None;
        }
        let box_end = offset.checked_add(box_size)?.min(end);
        let body = offset + header_len;
        let children_at = match kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => Some(body),
            b"stsd" => Some(body + STSD_HEADER_BYTES),
            b"Opus" => Some(body + AUDIO_SAMPLE_ENTRY_BYTES),
            b"dOps" => {
                let mut fields = [0u8; 2];
                reader.read_exact(&mut fields).ok()?;
                let [version, channel_count] = fields;
                if version == 0 && channel_count > 0 {
                    return Some(channel_count);
                }
                None
            }
            _ => None,
        };
        if let Some(children_at) = children_at
            && let Some(count) = find_dops_channel_count(reader, children_at, box_end, depth + 1)
        {
            return Some(count);
        }
        offset = box_end;
    }
    None
}

fn timestamp_to_100ns(time_base: TimeBase, timestamp: symphonia::core::units::Timestamp) -> i64 {
    (time_base.calc_time_saturating(timestamp).as_nanos() / 100) as i64
}

#[cfg(test)]
mod tests {
    use super::find_dops_channel_count;

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn dops_is_found_through_the_box_tree_without_reading_media_payload() {
        let dops = mp4_box(b"dOps", &[0, 6, 0x01, 0x38, 0, 0, 0xbb, 0x80, 0, 0, 1]);
        let mut opus_body = vec![0u8; 28];
        opus_body.extend_from_slice(&dops);
        let opus = mp4_box(b"Opus", &opus_body);
        let mut stsd_body = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd_body.extend_from_slice(&opus);
        let tree = mp4_box(
            b"moov",
            &mp4_box(
                b"trak",
                &mp4_box(
                    b"mdia",
                    &mp4_box(b"minf", &mp4_box(b"stbl", &mp4_box(b"stsd", &stsd_body))),
                ),
            ),
        );
        // Media payload first, containing a decoy 'dOps' byte pattern that a
        // byte scan would match with the wrong channel count.
        let mut file = mp4_box(b"ftyp", b"isom");
        let mut payload = vec![0u8; 4096];
        payload[100..106].copy_from_slice(&[b'd', b'O', b'p', b's', 0, 2]);
        file.extend_from_slice(&mp4_box(b"mdat", &payload));
        file.extend_from_slice(&tree);
        let len = file.len() as u64;
        let mut reader = std::io::Cursor::new(file);
        assert_eq!(find_dops_channel_count(&mut reader, 0, len, 0), Some(6));
    }

    #[test]
    fn truncated_or_box_free_input_yields_none() {
        let mut reader = std::io::Cursor::new(vec![0u8; 3]);
        assert_eq!(find_dops_channel_count(&mut reader, 0, 3, 0), None);
        let bogus = mp4_box(b"moov", &[0, 0, 0, 2, b'x', b'x', b'x', b'x']);
        let len = bogus.len() as u64;
        let mut reader = std::io::Cursor::new(bogus);
        assert_eq!(find_dops_channel_count(&mut reader, 0, len, 0), None);
    }
}
