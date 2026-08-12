use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::state::{CurrentClip, RecentClip};
use crate::api::tts::types::TtsCollectedAudio;
use crate::config::TtsMethod;

#[derive(Serialize, Deserialize)]
struct StoredClip {
    id: String,
    text: String,
    method: TtsMethod,
    voice_label: String,
    sample_rate: u32,
    duration_ms: u64,
    created_label: String,
    wav_file: String,
}

pub(super) fn load_recent(limit: usize) -> Vec<(CurrentClip, TtsCollectedAudio)> {
    let (_, db_path, dir) = paths();
    let clips: Vec<StoredClip> = std::fs::read_to_string(&db_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    clips
        .into_iter()
        .filter_map(|clip| {
            let wav_data = std::fs::read(owned_clip_path(&dir, &clip.wav_file)?).ok()?;
            let pcm_samples = decode_wav_to_24khz_mono(&wav_data).ok()?;
            let duration_sec = clip.duration_ms as f32 / 1000.0;
            let current = CurrentClip {
                id: clip.id,
                text: clip.text,
                voice_label: clip.voice_label,
                created_label: clip.created_label,
                duration_sec,
                sample_rate: clip.sample_rate,
            };
            let audio = TtsCollectedAudio {
                wav_data,
                pcm_samples,
                sample_rate: clip.sample_rate,
                duration_ms: clip.duration_ms,
            };
            Some((current, audio))
        })
        .take(limit)
        .collect()
}

pub(super) fn save_recent(
    clips: &[(RecentClip, std::sync::Arc<TtsCollectedAudio>)],
    methods: &[(String, TtsMethod)],
) {
    let (_, db_path, dir) = paths();
    let _ = std::fs::create_dir_all(&dir);
    let previous: Vec<StoredClip> = std::fs::read_to_string(&db_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut stored = Vec::new();
    for (clip, audio) in clips {
        let wav_file = format!("{}.wav", safe_file_stem(&clip.id));
        if std::fs::write(dir.join(&wav_file), &audio.wav_data).is_err() {
            continue;
        }
        stored.push(StoredClip {
            id: clip.id.clone(),
            text: clip.text.clone(),
            method: methods
                .iter()
                .find(|(id, _)| id == &clip.id)
                .map(|(_, method)| method.clone())
                .unwrap_or_default(),
            voice_label: clip.voice_label.clone(),
            sample_rate: audio.sample_rate,
            duration_ms: audio.duration_ms,
            created_label: clip.created_label.clone(),
            wav_file,
        });
    }
    if crate::atomic_json::write_json_atomic(&db_path, &stored).is_ok() {
        let retained = stored
            .iter()
            .map(|clip| clip.wav_file.as_str())
            .collect::<BTreeSet<_>>();
        for clip in previous {
            if !retained.contains(clip.wav_file.as_str())
                && let Some(path) = owned_clip_path(&dir, &clip.wav_file)
            {
                delete_regular_file(&path);
            }
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if !retained.contains(name)
                    && let Some(path) =
                        owned_clip_path(&dir, name).or_else(|| owned_legacy_clip_path(&dir, name))
                {
                    delete_regular_file(&path);
                }
            }
        }
    }
}

pub(super) fn save_managed_wav(prefix: &str, wav_data: &[u8]) -> Result<PathBuf, String> {
    let (_, _, dir) = paths();
    let id = chrono::Local::now()
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .unsigned_abs();
    let path = dir.join(format!("{}_{}.wav", safe_file_stem(prefix), id));
    std::fs::write(&path, wav_data).map_err(|err| err.to_string())?;
    Ok(path)
}

pub(super) fn encode_managed_wav(
    prefix: &str,
    samples: &[i16],
    sample_rate: u32,
) -> Result<PathBuf, String> {
    let wav_data = crate::api::audio::encode_wav(samples, sample_rate, 1);
    save_managed_wav(prefix, &wav_data)
}

pub(super) fn delete_replaced_managed_audio(previous: &str, replacement: &Path) {
    if !previous.is_empty() && Path::new(previous) != replacement {
        delete_managed_audio(previous);
    }
}

pub(super) fn delete_managed_audio(raw: &str) {
    let path = Path::new(raw);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let owned_playground_wav = path.parent() == Some(paths().2.as_path())
        && (file_name.starts_with("mic_") || file_name.starts_with("reference-mic_"));
    let owned_temp_wav = path.parent() == Some(std::env::temp_dir().as_path())
        && file_name.starts_with("sgt-tts-source-");
    if path.extension().and_then(|value| value.to_str()) == Some("wav")
        && (owned_playground_wav || owned_temp_wav)
    {
        delete_regular_file(path);
    }
}

pub(super) fn prune_managed_audio(retained: &[String]) {
    let retained = retained.iter().map(PathBuf::from).collect::<BTreeSet<_>>();
    for root in [paths().2, std::env::temp_dir()] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if retained.contains(&path) {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("mic_")
                || name.starts_with("reference-mic_")
                || name.starts_with("sgt-tts-source-")
            {
                delete_managed_audio(path.to_string_lossy().as_ref());
            }
        }
    }
}

pub(super) fn decode_wav_to_24khz_mono(wav_data: &[u8]) -> Result<Vec<i16>, String> {
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
        Ok(crate::api::tts::worker::resample_audio(
            &mono,
            spec.sample_rate,
            crate::api::tts::types::SOURCE_SAMPLE_RATE,
        ))
    }
}

fn paths() -> (PathBuf, PathBuf, PathBuf) {
    let config_dir = crate::paths::app_config_dir();
    let dir = config_dir.join("tts_playground");
    let db_path = dir.join("clips.json");
    let _ = std::fs::create_dir_all(&dir);
    (config_dir, db_path, dir)
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn owned_clip_path(dir: &Path, file_name: &str) -> Option<PathBuf> {
    let path = Path::new(file_name);
    let id = file_name.strip_prefix("clip-")?.strip_suffix(".wav")?;
    if path.file_name().and_then(|value| value.to_str()) != Some(file_name)
        || id.is_empty()
        || !id.bytes().all(|value| value.is_ascii_digit())
        || path.extension().and_then(|value| value.to_str()) != Some("wav")
    {
        return None;
    }
    Some(dir.join(file_name))
}

fn owned_legacy_clip_path(dir: &Path, file_name: &str) -> Option<PathBuf> {
    let path = Path::new(file_name);
    let timestamp = file_name.strip_prefix("clip_")?.strip_suffix(".wav")?;
    if path.file_name().and_then(|value| value.to_str()) != Some(file_name)
        || timestamp.len() < 10
        || timestamp.len() > 20
        || !timestamp.bytes().all(|value| value.is_ascii_digit())
    {
        return None;
    }
    Some(dir.join(file_name))
}

fn delete_regular_file(path: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::{owned_clip_path, owned_legacy_clip_path};
    use std::path::Path;

    #[test]
    fn clip_cleanup_accepts_only_exact_app_owned_names() {
        let root = Path::new("tts-playground");
        assert_eq!(
            owned_clip_path(root, "clip-42.wav"),
            Some(root.join("clip-42.wav"))
        );
        for unsafe_name in [
            "../clip-42.wav",
            "clip-name.wav",
            "clip-42.mp3",
            "reference-mic_42.wav",
            "clip-42.wav.bak",
        ] {
            assert_eq!(owned_clip_path(root, unsafe_name), None);
        }
    }

    #[test]
    fn legacy_cleanup_accepts_only_the_retired_timestamp_name() {
        let root = Path::new("tts-playground");
        assert_eq!(
            owned_legacy_clip_path(root, "clip_1779116029141.wav"),
            Some(root.join("clip_1779116029141.wav"))
        );
        for unsafe_name in [
            "../clip_1779116029141.wav",
            "clip_short.wav",
            "clip_1779116029141.mp3",
            "clip_reference.wav",
            "clip_1779116029141.wav.bak",
        ] {
            assert_eq!(owned_legacy_clip_path(root, unsafe_name), None);
        }
    }
}
