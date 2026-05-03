use super::state::{MAX_RECENT_ARTIFACTS, TtsPlaygroundArtifact};
use crate::config::TtsMethod;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct StoredClip {
    id: u64,
    text: String,
    method: TtsMethod,
    voice_label: String,
    sample_rate: u32,
    duration_ms: u64,
    latency_ms: u128,
    created_label: String,
    wav_file: String,
}

pub(super) fn load_recent() -> Vec<TtsPlaygroundArtifact> {
    let (_, db_path, dir) = paths();
    let clips: Vec<StoredClip> = std::fs::read_to_string(&db_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    clips
        .into_iter()
        .filter_map(|clip| {
            let wav_data = std::fs::read(dir.join(&clip.wav_file)).ok()?;
            let pcm_samples = decode_wav_to_24khz_mono(&wav_data).ok()?;
            Some(TtsPlaygroundArtifact {
                id: clip.id,
                text: clip.text,
                method: clip.method,
                voice_label: clip.voice_label,
                pcm_samples,
                wav_data,
                sample_rate: clip.sample_rate,
                duration_ms: clip.duration_ms,
                latency_ms: clip.latency_ms,
                created_label: clip.created_label,
            })
        })
        .take(MAX_RECENT_ARTIFACTS)
        .collect()
}

pub(super) fn save_recent(clips: &std::collections::VecDeque<TtsPlaygroundArtifact>) {
    let (_, db_path, dir) = paths();
    let _ = std::fs::create_dir_all(&dir);
    let mut stored = Vec::new();
    for artifact in clips.iter().take(MAX_RECENT_ARTIFACTS) {
        let wav_file = format!("clip_{}.wav", artifact.id);
        if std::fs::write(dir.join(&wav_file), &artifact.wav_data).is_err() {
            continue;
        }
        stored.push(StoredClip {
            id: artifact.id,
            text: artifact.text.clone(),
            method: artifact.method.clone(),
            voice_label: artifact.voice_label.clone(),
            sample_rate: artifact.sample_rate,
            duration_ms: artifact.duration_ms,
            latency_ms: artifact.latency_ms,
            created_label: artifact.created_label.clone(),
            wav_file,
        });
    }
    if let Ok(json) = serde_json::to_string_pretty(&stored) {
        let _ = std::fs::write(db_path, json);
    }
}

fn paths() -> (PathBuf, PathBuf, PathBuf) {
    let config_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
    let dir = config_dir.join("tts_playground");
    let db_path = dir.join("clips.json");
    let _ = std::fs::create_dir_all(&dir);
    (config_dir, db_path, dir)
}

fn decode_wav_to_24khz_mono(wav_data: &[u8]) -> Result<Vec<i16>, String> {
    let cursor = std::io::Cursor::new(wav_data);
    let reader = hound::WavReader::new(cursor).map_err(|err| err.to_string())?;
    let spec = reader.spec();

    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .into_samples::<i16>()
            .filter_map(Result::ok)
            .collect(),
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(Result::ok)
            .map(|sample| (sample * i16::MAX as f32).clamp(-32768.0, 32767.0) as i16)
            .collect(),
    };

    let mono = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|sample| *sample as i32).sum();
                (sum / chunk.len() as i32) as i16
            })
            .collect()
    } else {
        samples
    };

    if spec.sample_rate == crate::api::tts::types::SOURCE_SAMPLE_RATE {
        Ok(mono)
    } else {
        Ok(resample_linear(
            &mono,
            spec.sample_rate,
            crate::api::tts::types::SOURCE_SAMPLE_RATE,
        ))
    }
}

fn resample_linear(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if samples.is_empty() || from_rate == to_rate {
        return samples.to_vec();
    }
    let ratio = to_rate as f32 / from_rate as f32;
    let new_len = (samples.len() as f32 * ratio) as usize;
    let mut output = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src = i as f32 / ratio;
        let idx = src as usize;
        if idx >= samples.len().saturating_sub(1) {
            output.push(*samples.last().unwrap_or(&0));
        } else {
            let frac = src - idx as f32;
            let a = samples[idx] as f32;
            let b = samples[idx + 1] as f32;
            output.push((a + (b - a) * frac) as i16);
        }
    }
    output
}
