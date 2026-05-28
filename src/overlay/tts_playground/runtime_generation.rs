use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::{TtsCollectedAudio, TtsRequestProfile};
use crate::config::TtsMethod;

use super::runtime::{
    CANCEL_FLAG, PLAYBACK_TIMER, RECENT_LIMIT, cache_method, cache_put, describe_voice,
    next_clip_id, persist_recent,
};
use super::runtime_playback::start_pcm_playback;
use super::state::{self, CurrentClip, RecentClip};

pub(super) fn start_generation() {
    let mode = match crate::APP.lock() {
        Ok(app) => app.config.tts_playground.mode.clone(),
        Err(_) => return,
    };
    if matches!(mode, crate::config::TtsPlaygroundMode::AudioEdit) {
        start_audio_edit_generation();
        return;
    }
    if matches!(mode, crate::config::TtsPlaygroundMode::SpeechToSpeech) {
        start_s2s_generation();
        return;
    }

    let (text, profile) = match crate::APP.lock() {
        Ok(app) => (
            app.config.tts_playground.draft_text.clone(),
            TtsRequestProfile::from(&app.config.tts_playground),
        ),
        Err(_) => return,
    };
    if text.trim().is_empty() {
        state::with_state(|s| {
            s.error = Some("Type some text first.".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    if matches!(profile.method, TtsMethod::StepAudioEditX)
        && let Some(issue) = crate::config::step_audio_tts_text_issue(&text)
    {
        state::with_state(|s| {
            s.error = Some(issue.to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = CANCEL_FLAG.lock().unwrap();
        if let Some(prev) = slot.take() {
            prev.store(true, Ordering::SeqCst);
        }
        *slot = Some(cancel.clone());
    }

    state::with_state(|s| {
        s.is_generating = true;
        s.error = None;
        s.status = "Generating…".to_string();
    });
    state::sync_to_webview();

    let method = profile.method.clone();
    let voice_label = describe_voice(method.clone());
    let text_for_thread = text.clone();
    std::thread::spawn(move || {
        let started_at = Instant::now();
        let result = TTS_MANAGER.synthesize_to_wav_with_profile_cancel(
            &text_for_thread,
            profile,
            cancel.clone(),
        );
        match result {
            Ok(audio) => {
                let id = next_clip_id();
                let clip = CurrentClip {
                    id: id.clone(),
                    text: text_for_thread.clone(),
                    voice_label: voice_label.clone(),
                    created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
                    duration_sec: audio.duration_ms as f32 / 1000.0,
                    sample_rate: audio.sample_rate,
                };
                cache_put(id.clone(), audio.clone());
                cache_method(id.clone(), method);
                let total_samples = audio.pcm_samples.len();
                let sample_rate = audio.sample_rate;
                state::with_state(|s| {
                    s.recent.retain(|item| item.id != id);
                    s.recent.insert(
                        0,
                        RecentClip {
                            id: clip.id.clone(),
                            text: clip.text.clone(),
                            voice_label: clip.voice_label.clone(),
                            created_label: clip.created_label.clone(),
                            duration_sec: clip.duration_sec,
                        },
                    );
                    s.recent.truncate(RECENT_LIMIT);
                    s.current = Some(clip);
                    s.is_generating = false;
                    s.error = None;
                    let latency = started_at.elapsed().as_millis();
                    s.status = format!("Generated in {latency}ms");
                    s.position_sec = 0.0;
                    s.is_playing = true;
                    s.paused = false;
                });
                persist_recent();
                // Kick off playback from sample 0
                start_pcm_playback(audio.pcm_samples.clone(), sample_rate, total_samples, 0);
                state::sync_to_webview();
            }
            Err(err) => {
                state::with_state(|s| {
                    s.is_generating = false;
                    s.error = Some(err.to_string());
                    s.status.clear();
                });
                state::sync_to_webview();
            }
        }
    });
}

fn start_audio_edit_generation() {
    let settings = match crate::APP.lock() {
        Ok(app) => app.config.tts_playground.step_audio_edit_settings.clone(),
        Err(_) => return,
    };
    if settings.source_audio_path.trim().is_empty() {
        state::with_state(|s| {
            s.error = Some("Pick a source audio file first.".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    if settings.source_text.trim().is_empty() {
        state::with_state(|s| {
            s.error = Some("Type the source transcript first.".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    if settings.edit_type == "paralinguistic" && settings.target_text.trim().is_empty() {
        state::with_state(|s| {
            s.error = Some("Type the target text first.".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = CANCEL_FLAG.lock().unwrap();
        if let Some(prev) = slot.take() {
            prev.store(true, Ordering::SeqCst);
        }
        *slot = Some(cancel.clone());
    }
    state::with_state(|s| {
        s.is_generating = true;
        s.error = None;
        s.status = "Editing audio…".to_string();
    });
    state::sync_to_webview();

    std::thread::spawn(move || {
        let started_at = Instant::now();
        let result = crate::api::tts::worker::synthesize_step_audio_edit_to_wav_cancel(
            settings.source_audio_path.clone(),
            settings.source_text.clone(),
            settings.edit_type.clone(),
            settings.edit_info.clone(),
            settings.target_text.clone(),
            cancel,
        );
        match result {
            Ok(audio) => {
                let id = next_clip_id();
                let label = format!("Audio Edit · {}", settings.edit_type);
                let clip = CurrentClip {
                    id: id.clone(),
                    text: settings.target_text.clone(),
                    voice_label: label.clone(),
                    created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
                    duration_sec: audio.duration_ms as f32 / 1000.0,
                    sample_rate: audio.sample_rate,
                };
                cache_put(id.clone(), audio.clone());
                cache_method(id.clone(), TtsMethod::StepAudioEditX);
                let total = audio.pcm_samples.len();
                let sample_rate = audio.sample_rate;
                state::with_state(|s| {
                    s.recent.retain(|item| item.id != id);
                    s.recent.insert(
                        0,
                        RecentClip {
                            id: clip.id.clone(),
                            text: clip.text.clone(),
                            voice_label: clip.voice_label.clone(),
                            created_label: clip.created_label.clone(),
                            duration_sec: clip.duration_sec,
                        },
                    );
                    s.recent.truncate(RECENT_LIMIT);
                    s.current = Some(clip);
                    s.is_generating = false;
                    s.status = format!("Edited in {}ms", started_at.elapsed().as_millis());
                    s.position_sec = 0.0;
                    s.is_playing = true;
                    s.paused = false;
                });
                persist_recent();
                start_pcm_playback(audio.pcm_samples.clone(), sample_rate, total, 0);
                state::sync_to_webview();
            }
            Err(err) => {
                state::with_state(|s| {
                    s.is_generating = false;
                    s.error = Some(err.to_string());
                });
                state::sync_to_webview();
            }
        }
    });
}

fn start_s2s_generation() {
    let (source_path, target_language, model, voice, speed) = match crate::APP.lock() {
        Ok(app) => (
            app.config
                .tts_playground
                .step_audio_edit_settings
                .source_audio_path
                .trim()
                .to_string(),
            app.config.realtime_target_language.clone(),
            app.config.tts_playground.gemini_model.clone(),
            app.config.tts_playground.gemini_voice.clone(),
            app.config.tts_playground.gemini_speed.clone(),
        ),
        Err(_) => return,
    };
    if source_path.is_empty() {
        state::with_state(|s| {
            s.error = Some("Pick or record source audio first.".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    let settings = match crate::api::realtime_audio::s2s::default_batch_settings_for_target(
        &target_language,
        &model,
        &voice,
        &speed,
    ) {
        Ok(settings) => settings,
        Err(error) => {
            state::with_state(|s| s.error = Some(error.to_string()));
            state::sync_to_webview();
            return;
        }
    };

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = CANCEL_FLAG.lock().unwrap();
        if let Some(prev) = slot.take() {
            prev.store(true, Ordering::SeqCst);
        }
        *slot = Some(cancel.clone());
    }
    state::with_state(|s| {
        s.is_generating = true;
        s.error = None;
        s.status = "Generating Gemini S2S audio...".to_string();
    });
    state::sync_to_webview();

    std::thread::spawn(move || {
        let started_at = Instant::now();
        let result = decode_audio_16k_mono(&source_path)
            .and_then(|samples| {
                crate::api::realtime_audio::s2s::run_gemini_live_s2s_batch(
                    samples, settings, cancel,
                )
                .map_err(|error| error.to_string())
            })
            .and_then(|segments| {
                let mut pcm_samples = Vec::new();
                let mut source_text = Vec::new();
                let mut target_text = Vec::new();
                for segment in segments {
                    if !segment.source_text.trim().is_empty() {
                        source_text.push(segment.source_text);
                    }
                    if !segment.target_text.trim().is_empty() {
                        target_text.push(segment.target_text);
                    }
                    pcm_samples.extend(segment.audio_pcm_24k);
                }
                if pcm_samples.is_empty() {
                    return Err("Gemini S2S returned no audio".to_string());
                }
                let wav_data = crate::api::audio::encode_wav(&pcm_samples, 24_000, 1);
                let duration_ms = ((pcm_samples.len() as f64 / 24_000.0) * 1000.0).round() as u64;
                Ok(TtsCollectedAudio {
                    pcm_samples,
                    wav_data,
                    sample_rate: 24_000,
                    duration_ms,
                }
                .with_text(format!(
                    "Source: {}\nTarget: {}",
                    source_text.join(" "),
                    target_text.join(" ")
                )))
            });

        match result {
            Ok((audio, text)) => {
                let id = next_clip_id();
                let clip = CurrentClip {
                    id: id.clone(),
                    text,
                    voice_label: format!("Gemini S2S · {voice}"),
                    created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
                    duration_sec: audio.duration_ms as f32 / 1000.0,
                    sample_rate: audio.sample_rate,
                };
                cache_put(id.clone(), audio.clone());
                cache_method(id.clone(), TtsMethod::GeminiLive);
                let total = audio.pcm_samples.len();
                let sample_rate = audio.sample_rate;
                state::with_state(|s| {
                    s.recent.retain(|item| item.id != id);
                    s.recent.insert(
                        0,
                        RecentClip {
                            id: clip.id.clone(),
                            text: clip.text.clone(),
                            voice_label: clip.voice_label.clone(),
                            created_label: clip.created_label.clone(),
                            duration_sec: clip.duration_sec,
                        },
                    );
                    s.recent.truncate(RECENT_LIMIT);
                    s.current = Some(clip);
                    s.is_generating = false;
                    s.error = None;
                    s.status = format!("Generated S2S in {}ms", started_at.elapsed().as_millis());
                    s.position_sec = 0.0;
                    s.is_playing = true;
                    s.paused = false;
                });
                persist_recent();
                start_pcm_playback(audio.pcm_samples.clone(), sample_rate, total, 0);
            }
            Err(err) => {
                state::with_state(|s| {
                    s.is_generating = false;
                    s.error = Some(err);
                    s.status.clear();
                });
            }
        }
        state::sync_to_webview();
    });
}

trait AudioWithText {
    fn with_text(self, text: String) -> (Self, String)
    where
        Self: Sized;
}

impl AudioWithText for TtsCollectedAudio {
    fn with_text(self, text: String) -> (Self, String) {
        (self, text)
    }
}

fn decode_audio_16k_mono(path: &str) -> Result<Vec<i16>, String> {
    let decoder = crate::overlay::screen_record::mf_audio::MfAudioDecoder::new_with_output_format(
        path,
        Some(16_000),
        Some(1),
    )?;
    let channels = decoder.channels().max(1) as usize;
    let mut samples = Vec::new();
    while let Some((bytes, _)) = decoder.read_samples()? {
        let floats = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect::<Vec<_>>();
        for frame in floats.chunks(channels) {
            let sample = frame.first().copied().unwrap_or(0.0).clamp(-1.0, 1.0);
            samples.push((sample * i16::MAX as f32) as i16);
        }
    }
    Ok(samples)
}

pub(super) fn cancel_generation() {
    if let Some(cancel) = CANCEL_FLAG.lock().unwrap().as_ref() {
        cancel.store(true, Ordering::SeqCst);
    }
    TTS_MANAGER.stop();
    state::with_state(|s| {
        s.is_generating = false;
        s.is_playing = false;
        s.status = "Cancelled".to_string();
    });
    *PLAYBACK_TIMER.lock().unwrap() = None;
    state::sync_to_webview();
}

pub(super) fn clear_current() {
    if let Ok(mut app) = crate::APP.lock() {
        app.config.tts_playground.draft_text.clear();
        crate::config::save_config(&app.config);
    }
    state::with_state(|s| s.error = None);
    state::sync_to_webview();
}
