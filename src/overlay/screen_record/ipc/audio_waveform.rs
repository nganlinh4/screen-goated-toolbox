use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::UNIX_EPOCH;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::overlay::screen_record::mf_audio::MfAudioDecoder;

const WAVEFORM_CACHE_MAGIC: &[u8; 5] = b"SGWF1";
const WAVEFORM_CACHE_VERSION: u32 = 1;
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

pub fn handle_get_audio_waveform(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: AudioWaveformRequest = serde_json::from_value(args.clone())
        .map_err(|err| format!("Decode audio waveform request: {err}"))?;
    let response = get_audio_waveform(&request)?;
    serde_json::to_value(response).map_err(|err| format!("Serialize audio waveform response: {err}"))
}

fn get_audio_waveform(
    request: &AudioWaveformRequest,
) -> Result<AudioWaveformResponse, String> {
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
    let fingerprint = format!(
        "{}|{}|{}",
        normalize_path(path),
        metadata.len(),
        modified
    );
    let cache_key = hex_digest(&fingerprint);

    if let Some(cached) = memory_cache().lock().get(&cache_key) {
        return Ok(cached);
    }

    if let Some(disk_cached) = load_envelope_from_disk(&cache_key)? {
        let disk_cached = Arc::new(disk_cached);
        memory_cache()
            .lock()
            .insert(cache_key.clone(), Arc::clone(&disk_cached));
        return Ok(disk_cached);
    }

    let analyzed = Arc::new(analyze_source_waveform(path)?);
    memory_cache()
        .lock()
        .insert(cache_key.clone(), Arc::clone(&analyzed));
    if let Err(err) = save_envelope_to_disk(&cache_key, &analyzed) {
        eprintln!("[Waveform] Failed to persist cache {}: {}", path.display(), err);
    }
    Ok(analyzed)
}

fn analyze_source_waveform(path: &Path) -> Result<SourceWaveformEnvelope, String> {
    let decoder = MfAudioDecoder::new(&path.to_string_lossy())?;
    let sample_rate = decoder.sample_rate().max(1);
    let channels = decoder.channels().max(1) as usize;
    let mut bins: Vec<QuantizedWaveformBin> = Vec::new();
    let mut frame_cursor = 0u64;

    while let Some((pcm_bytes, _timestamp)) = decoder.read_samples()? {
        if pcm_bytes.is_empty() {
            continue;
        }
        ingest_pcm_chunk(
            &pcm_bytes,
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

        let bin_index = ((*frame_cursor) * SOURCE_BINS_PER_SECOND as u64 / sample_rate as u64)
            as usize;
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

    let source_bin_duration = 1.0 / SOURCE_BINS_PER_SECOND as f64;
    let target_bin_duration = envelope.source_duration_sec / target_bins as f64;
    let mut output = Vec::with_capacity(target_bins);

    for target_index in 0..target_bins {
        let target_start = target_index as f64 * target_bin_duration;
        let target_end = if target_index + 1 >= target_bins {
            envelope.source_duration_sec
        } else {
            (target_index + 1) as f64 * target_bin_duration
        };

        let source_start = (target_start / source_bin_duration).floor() as usize;
        let source_end = ((target_end / source_bin_duration).ceil() as usize)
            .min(envelope.bins.len());

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
    dirs::data_local_dir().map(|base| {
        base.join("screen-goated-toolbox")
            .join("screen-record-waveforms")
    })
}

fn waveform_cache_path(cache_key: &str) -> Option<PathBuf> {
    waveform_cache_dir().map(|dir| dir.join(format!("{cache_key}.bin")))
}

fn load_envelope_from_disk(
    cache_key: &str,
) -> Result<Option<SourceWaveformEnvelope>, String> {
    let Some(path) = waveform_cache_path(cache_key) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let file = File::open(&path)
        .map_err(|err| format!("Open waveform cache {}: {err}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut magic = [0_u8; 5];
    reader
        .read_exact(&mut magic)
        .map_err(|err| format!("Read waveform cache magic {}: {err}", path.display()))?;
    if &magic != WAVEFORM_CACHE_MAGIC {
        return Ok(None);
    }

    let version = reader
        .read_u32::<LittleEndian>()
        .map_err(|err| format!("Read waveform cache version {}: {err}", path.display()))?;
    if version != WAVEFORM_CACHE_VERSION {
        return Ok(None);
    }

    let source_duration_sec = reader
        .read_f64::<LittleEndian>()
        .map_err(|err| format!("Read waveform duration {}: {err}", path.display()))?;
    let bin_count = reader
        .read_u32::<LittleEndian>()
        .map_err(|err| format!("Read waveform bin count {}: {err}", path.display()))?
        as usize;

    let mut bins = Vec::with_capacity(bin_count);
    for _ in 0..bin_count {
        let min = reader
            .read_i16::<LittleEndian>()
            .map_err(|err| format!("Read waveform bin min {}: {err}", path.display()))?;
        let max = reader
            .read_i16::<LittleEndian>()
            .map_err(|err| format!("Read waveform bin max {}: {err}", path.display()))?;
        bins.push(QuantizedWaveformBin { min, max });
    }

    Ok(Some(SourceWaveformEnvelope {
        source_duration_sec,
        bins,
    }))
}

fn save_envelope_to_disk(
    cache_key: &str,
    envelope: &SourceWaveformEnvelope,
) -> Result<(), String> {
    let Some(path) = waveform_cache_path(cache_key) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Create waveform cache dir {}: {err}", parent.display()))?;
    }

    let file = File::create(&path)
        .map_err(|err| format!("Create waveform cache {}: {err}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(WAVEFORM_CACHE_MAGIC)
        .map_err(|err| format!("Write waveform cache magic {}: {err}", path.display()))?;
    writer
        .write_u32::<LittleEndian>(WAVEFORM_CACHE_VERSION)
        .map_err(|err| format!("Write waveform cache version {}: {err}", path.display()))?;
    writer
        .write_f64::<LittleEndian>(envelope.source_duration_sec)
        .map_err(|err| format!("Write waveform duration {}: {err}", path.display()))?;
    writer
        .write_u32::<LittleEndian>(envelope.bins.len() as u32)
        .map_err(|err| format!("Write waveform bin count {}: {err}", path.display()))?;
    for bin in &envelope.bins {
        writer
            .write_i16::<LittleEndian>(bin.min)
            .map_err(|err| format!("Write waveform bin min {}: {err}", path.display()))?;
        writer
            .write_i16::<LittleEndian>(bin.max)
            .map_err(|err| format!("Write waveform bin max {}: {err}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|err| format!("Flush waveform cache {}: {err}", path.display()))
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
    use super::{QuantizedWaveformBin, SourceWaveformEnvelope, resample_envelope};

    #[test]
    fn resample_envelope_preserves_peaks() {
        let envelope = SourceWaveformEnvelope {
            source_duration_sec: 1.0,
            bins: vec![
                QuantizedWaveformBin { min: -1000, max: 2000 },
                QuantizedWaveformBin { min: -5000, max: 3000 },
                QuantizedWaveformBin { min: -4000, max: 12000 },
                QuantizedWaveformBin { min: -300, max: 400 },
            ],
        };

        let output = resample_envelope(&envelope, 2);
        assert_eq!(output.len(), 2);
        assert!(output[0].min < -0.1);
        assert!(output[0].max > 0.09);
        assert!(output[1].max > 0.35);
    }
}
