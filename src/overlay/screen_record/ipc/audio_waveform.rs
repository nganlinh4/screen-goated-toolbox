use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::UNIX_EPOCH;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::overlay::screen_record::mf_audio::MfAudioDecoder;

#[path = "audio_waveform/store.rs"]
mod store;

const WAVEFORM_CACHE_MAGIC: &[u8; 5] = b"SGWF1";
// 2: decoded chunks are placed at their timestamps.
const WAVEFORM_CACHE_VERSION: u32 = 2;
const SOURCE_BINS_PER_SECOND: u32 = 240;
const MAX_MEMORY_CACHE_ENTRIES: usize = 12;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioWaveformRequest {
    pub path: String,
    pub target_bins: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioWaveformResponse {
    pub bins: Vec<AudioWaveformBin>,
    pub source_duration_sec: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioWaveformBin {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug)]
struct QuantizedWaveformBin {
    min: i16,
    max: i16,
}

impl QuantizedWaveformBin {
    fn silent() -> Self {
        Self { min: 0, max: 0 }
    }

    fn update(&mut self, sample: f32) {
        let quantized = quantize_sample(sample);
        self.min = self.min.min(quantized);
        self.max = self.max.max(quantized);
    }
}

#[derive(Clone, Debug)]
struct SourceWaveformEnvelope {
    source_duration_sec: f64,
    bins: Vec<QuantizedWaveformBin>,
}

#[derive(Default)]
struct WaveformMemoryCache {
    entries: HashMap<String, Arc<SourceWaveformEnvelope>>,
    order: VecDeque<String>,
}

impl WaveformMemoryCache {
    fn get(&mut self, key: &str) -> Option<Arc<SourceWaveformEnvelope>> {
        let entry = self.entries.get(key).cloned()?;
        self.touch(key);
        Some(entry)
    }

    fn insert(&mut self, key: String, value: Arc<SourceWaveformEnvelope>) {
        self.entries.insert(key.clone(), value);
        self.touch(&key);
        while self.entries.len() > MAX_MEMORY_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    fn touch(&mut self, key: &str) {
        if let Some(index) = self.order.iter().position(|existing| existing == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.to_string());
    }
}

static MEMORY_CACHE: OnceLock<Mutex<WaveformMemoryCache>> = OnceLock::new();

fn memory_cache() -> &'static Mutex<WaveformMemoryCache> {
    MEMORY_CACHE.get_or_init(|| Mutex::new(WaveformMemoryCache::default()))
}

pub fn handle_get_audio_waveform(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: AudioWaveformRequest = serde_json::from_value(args.clone())
        .map_err(|err| format!("Decode audio waveform request: {err}"))?;
    let response = get_audio_waveform(&request)?;
    serde_json::to_value(response)
        .map_err(|err| format!("Serialize audio waveform response: {err}"))
}

fn get_audio_waveform(request: &AudioWaveformRequest) -> Result<AudioWaveformResponse, String> {
    let trimmed_path = request.path.trim();
    if trimmed_path.is_empty() {
        return Ok(AudioWaveformResponse {
            bins: Vec::new(),
            source_duration_sec: 0.0,
        });
    }

    let envelope = load_source_envelope(Path::new(trimmed_path))?;
    let requested_bins = request.target_bins.clamp(16, 4096) as usize;
    let bins = resample_envelope(&envelope, requested_bins);

    Ok(AudioWaveformResponse {
        bins,
        source_duration_sec: envelope.source_duration_sec,
    })
}

fn load_source_envelope(path: &Path) -> Result<Arc<SourceWaveformEnvelope>, String> {
    let metadata = fs::metadata(path)
        .map_err(|err| format!("Read waveform metadata {}: {err}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let fingerprint = format!("{}|{}|{}", normalize_path(path), metadata.len(), modified);
    let cache_key = hex_digest(&fingerprint);

    if let Some(cached) = memory_cache().lock().get(&cache_key) {
        return Ok(cached);
    }

    let disk_path = waveform_cache_path(&cache_key);
    let (envelope, read) =
        store::load_or_analyze(disk_path.as_deref(), || analyze_source_waveform(path))?;
    let envelope = Arc::new(envelope);
    memory_cache()
        .lock()
        .insert(cache_key, Arc::clone(&envelope));
    if read != store::CacheRead::Hit
        && let Some(path) = disk_path
        && let Err(err) = store::save(&path, &envelope, read == store::CacheRead::Corrupt)
    {
        eprintln!(
            "[Waveform] Failed to persist cache {}: {}",
            path.display(),
            err
        );
    }
    Ok(envelope)
}

fn analyze_source_waveform(path: &Path) -> Result<SourceWaveformEnvelope, String> {
    let decoder = MfAudioDecoder::new(&path.to_string_lossy())?;
    let sample_rate = decoder.sample_rate().max(1);
    let channels = decoder.channels().max(1) as usize;
    let mut bins: Vec<QuantizedWaveformBin> = Vec::new();
    let mut frame_cursor = 0u64;

    while let Some((pcm_bytes, timestamp_100ns)) = decoder.read_samples()? {
        ingest_timed_chunk(
            &pcm_bytes,
            timestamp_100ns,
            channels,
            sample_rate,
            &mut frame_cursor,
            &mut bins,
        );
    }

    Ok(SourceWaveformEnvelope {
        source_duration_sec: frame_cursor as f64 / sample_rate as f64,
        bins,
    })
}

/// Places a decoded chunk at its timestamp, as playback and export do. A late
/// first chunk or a gap in the stream leaves silent bins instead of pulling
/// the rest of the waveform earlier. Jitter under a millisecond is ignored.
fn ingest_timed_chunk(
    pcm_bytes: &[u8],
    timestamp_100ns: i64,
    channels: usize,
    sample_rate: u32,
    frame_cursor: &mut u64,
    bins: &mut Vec<QuantizedWaveformBin>,
) {
    if pcm_bytes.is_empty() {
        return;
    }
    let timestamp_frame =
        (timestamp_100ns.max(0) as u128 * u128::from(sample_rate) / 10_000_000) as u64;
    if timestamp_frame > *frame_cursor + u64::from(sample_rate / 1_000) {
        *frame_cursor = timestamp_frame;
    }
    ingest_pcm_chunk(pcm_bytes, channels, sample_rate, frame_cursor, bins);
}

fn ingest_pcm_chunk(
    pcm_bytes: &[u8],
    channels: usize,
    sample_rate: u32,
    frame_cursor: &mut u64,
    bins: &mut Vec<QuantizedWaveformBin>,
) {
    if channels == 0 || pcm_bytes.len() < channels * 4 {
        return;
    }
    let sample_count = pcm_bytes.len() / 4;
    let frame_count = sample_count / channels;

    for frame_idx in 0..frame_count {
        let mut mono = 0.0f32;
        for channel_idx in 0..channels {
            let sample_idx = (frame_idx * channels + channel_idx) * 4;
            let sample = f32::from_le_bytes(
                pcm_bytes[sample_idx..sample_idx + 4]
                    .try_into()
                    .unwrap_or([0_u8; 4]),
            );
            mono += sample;
        }
        mono /= channels as f32;

        let bin_index =
            ((*frame_cursor) * SOURCE_BINS_PER_SECOND as u64 / sample_rate as u64) as usize;
        if bin_index >= bins.len() {
            bins.resize(bin_index + 1, QuantizedWaveformBin::silent());
        }
        bins[bin_index].update(mono);
        *frame_cursor += 1;
    }
}

fn resample_envelope(
    envelope: &SourceWaveformEnvelope,
    target_bins: usize,
) -> Vec<AudioWaveformBin> {
    if target_bins == 0 || envelope.source_duration_sec <= 0.0 || envelope.bins.is_empty() {
        return Vec::new();
    }

    let source_len = envelope.bins.len();
    let mut output = Vec::with_capacity(target_bins);

    for target_index in 0..target_bins {
        // Map each target bin to a proportional span of source bins. The source
        // envelope is a fixed-density series (SOURCE_BINS_PER_SECOND), so a span
        // proportional to the bin count matches a time-based span while avoiding
        // floating-point edge cases and staying correct for any envelope density.
        let source_start = target_index * source_len / target_bins;
        let source_end = if target_index + 1 >= target_bins {
            source_len
        } else {
            ((target_index + 1) * source_len / target_bins).max(source_start + 1)
        };

        let mut min = i16::MAX;
        let mut max = i16::MIN;
        for bin in envelope
            .bins
            .iter()
            .skip(source_start)
            .take(source_end.saturating_sub(source_start))
        {
            min = min.min(bin.min);
            max = max.max(bin.max);
        }

        if min == i16::MAX || max == i16::MIN {
            output.push(AudioWaveformBin { min: 0.0, max: 0.0 });
        } else {
            output.push(AudioWaveformBin {
                min: min as f32 / i16::MAX as f32,
                max: max as f32 / i16::MAX as f32,
            });
        }
    }

    output
}

fn waveform_cache_dir() -> Option<PathBuf> {
    super::super::runtime_paths::optional_local_data_dir()
        .map(|base| base.join("screen-record-waveforms"))
}

fn waveform_cache_path(cache_key: &str) -> Option<PathBuf> {
    waveform_cache_dir().map(|dir| dir.join(format!("{cache_key}.bin")))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn hex_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn quantize_sample(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::{
        QuantizedWaveformBin, SOURCE_BINS_PER_SECOND, SourceWaveformEnvelope, ingest_timed_chunk,
        resample_envelope,
    };

    fn mono_chunk(frames: usize, value: f32) -> Vec<u8> {
        (0..frames).flat_map(|_| value.to_le_bytes()).collect()
    }

    #[test]
    fn decoded_audio_lands_at_its_timestamp_like_playback() {
        // 0.5 s of audio at 0.5 s and 0.5 s at 2.0 s: playback is silent
        // before, between and after, so the waveform must be too.
        const RATE: u32 = 1_000;
        let (mut cursor, mut bins) = (0u64, Vec::new());
        let chunk = mono_chunk(500, 0.5);
        for start_100ns in [5_000_000, 20_000_000] {
            ingest_timed_chunk(&chunk, start_100ns, 1, RATE, &mut cursor, &mut bins);
        }
        assert_eq!(cursor, 2_500, "the envelope ends where the audio ends");
        let bin_at = |sec: f64| bins[(sec * f64::from(SOURCE_BINS_PER_SECOND)) as usize];
        assert_eq!(bin_at(0.25).max, 0, "nothing plays before the first chunk");
        assert!(bin_at(0.75).max > 0);
        assert_eq!(bin_at(1.5).max, 0, "the gap stays silent");
        assert!(bin_at(2.25).max > 0);
    }

    #[test]
    fn resample_envelope_preserves_peaks() {
        let envelope = SourceWaveformEnvelope {
            source_duration_sec: 1.0,
            bins: vec![
                QuantizedWaveformBin {
                    min: -1000,
                    max: 2000,
                },
                QuantizedWaveformBin {
                    min: -5000,
                    max: 3000,
                },
                QuantizedWaveformBin {
                    min: -4000,
                    max: 12000,
                },
                QuantizedWaveformBin {
                    min: -300,
                    max: 400,
                },
            ],
        };

        let output = resample_envelope(&envelope, 2);
        assert_eq!(output.len(), 2);
        assert!(output[0].min < -0.1);
        assert!(output[0].max > 0.09);
        assert!(output[1].max > 0.35);
    }
}
