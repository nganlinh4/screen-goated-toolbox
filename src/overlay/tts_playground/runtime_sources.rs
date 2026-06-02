use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::TtsMethod;
use crate::gui::locale::LocaleText;

use super::file_dialogs;
use super::runtime::{
    MIC_SESSION, MicSession, REFERENCE_MIC_TARGET, cache_get, describe_voice, random_preview_text,
    reference_transcription_preset,
};
use super::state;

pub(super) fn pick_source_audio() -> Result<Option<String>, String> {
    match file_dialogs::pick_audio_file_dialog()? {
        Some(path) => {
            let path_str = path.display().to_string();
            if let Ok(mut app) = crate::APP.lock() {
                app.config
                    .tts_playground
                    .step_audio_edit_settings
                    .source_audio_path = path_str.clone();
                crate::config::save_config(&app.config);
            }
            state::sync_to_webview();
            Ok(Some(path_str))
        }
        None => Ok(None),
    }
}

pub(super) fn start_mic_recording() {
    *REFERENCE_MIC_TARGET.lock().unwrap() = None;
    let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let pause_flag = Arc::new(AtomicBool::new(false));
    let stream = match crate::api::realtime_audio::start_mic_capture(
        samples.clone(),
        stop_flag.clone(),
        pause_flag,
    ) {
        Ok(stream) => stream,
        Err(err) => {
            state::with_state(|s| {
                s.is_mic_recording = false;
                s.error = Some(format!("Mic unavailable: {err}"));
                s.status.clear();
            });
            state::sync_to_webview();
            return;
        }
    };
    let session = MicSession {
        samples,
        stop_flag,
        _stream: stream,
    };
    *MIC_SESSION.lock().unwrap() = Some(session);
    state::with_state(|s| {
        s.is_mic_recording = true;
        s.error = None;
        s.status = "Recording mic… click stop when done".to_string();
    });
    state::sync_to_webview();
}

pub(super) fn stop_mic_recording() {
    let session = MIC_SESSION.lock().unwrap().take();
    let Some(session) = session else {
        state::with_state(|s| s.is_mic_recording = false);
        state::sync_to_webview();
        return;
    };
    session.stop_flag.store(true, Ordering::SeqCst);
    let samples = session
        .samples
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    drop(session);
    if samples.is_empty() {
        state::with_state(|s| {
            s.is_mic_recording = false;
            s.error = Some("No audio captured".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    let path = match super::library::encode_managed_wav("mic", &samples, 16_000) {
        Ok(path) => path,
        Err(err) => {
            state::with_state(|s| {
                s.is_mic_recording = false;
                s.error = Some(format!("Failed to save mic clip: {err}"));
            });
            state::sync_to_webview();
            return;
        }
    };
    if !path.exists() {
        state::with_state(|s| {
            s.is_mic_recording = false;
            s.error = Some("Failed to save mic clip".to_string());
        });
        state::sync_to_webview();
        return;
    }
    if let Ok(mut app) = crate::APP.lock() {
        app.config
            .tts_playground
            .step_audio_edit_settings
            .source_audio_path = path.display().to_string();
        crate::config::save_config(&app.config);
    }
    let duration_sec = samples.len() as f32 / 16_000.0;
    state::with_state(|s| {
        s.is_mic_recording = false;
        s.error = None;
        s.status = format!("Mic clip saved ({duration_sec:.1}s) → AudioEdit source");
    });
    state::sync_to_webview();
}

pub(super) fn start_reference_mic(id: &str) {
    if id.trim().is_empty() {
        return;
    }
    if MIC_SESSION.lock().unwrap().is_some() {
        state::with_state(|s| {
            s.error = Some("Stop the current mic recording first.".to_string());
        });
        state::sync_to_webview();
        return;
    }
    *REFERENCE_MIC_TARGET.lock().unwrap() = Some(id.to_string());
    let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let pause_flag = Arc::new(AtomicBool::new(false));
    let stream = match crate::api::realtime_audio::start_mic_capture(
        samples.clone(),
        stop_flag.clone(),
        pause_flag,
    ) {
        Ok(stream) => stream,
        Err(err) => {
            *REFERENCE_MIC_TARGET.lock().unwrap() = None;
            state::with_state(|s| {
                s.is_mic_recording = false;
                s.error = Some(format!("Mic unavailable: {err}"));
                s.status.clear();
            });
            state::sync_to_webview();
            return;
        }
    };
    *MIC_SESSION.lock().unwrap() = Some(MicSession {
        samples,
        stop_flag,
        _stream: stream,
    });
    state::with_state(|s| {
        s.is_mic_recording = true;
        s.error = None;
        s.status = "Recording reference mic… click stop when done".to_string();
    });
    state::sync_to_webview();
}

pub(super) fn stop_reference_mic() {
    let target_id = REFERENCE_MIC_TARGET.lock().unwrap().take();
    let session = MIC_SESSION.lock().unwrap().take();
    let Some(target_id) = target_id else {
        state::with_state(|s| s.is_mic_recording = false);
        state::sync_to_webview();
        return;
    };
    let Some(session) = session else {
        state::with_state(|s| s.is_mic_recording = false);
        state::sync_to_webview();
        return;
    };
    session.stop_flag.store(true, Ordering::SeqCst);
    let samples = session
        .samples
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    drop(session);
    if samples.is_empty() {
        state::with_state(|s| {
            s.is_mic_recording = false;
            s.error = Some("No audio captured".to_string());
            s.status.clear();
        });
        state::sync_to_webview();
        return;
    }
    let path = match super::library::encode_managed_wav("reference-mic", &samples, 16_000) {
        Ok(path) => path,
        Err(err) => {
            state::with_state(|s| {
                s.is_mic_recording = false;
                s.error = Some(format!("Failed to save reference clip: {err}"));
            });
            state::sync_to_webview();
            return;
        }
    };
    let path_str = path.display().to_string();
    if let Ok(mut app) = crate::APP.lock()
        && let Some(reference) = app
            .config
            .step_audio_reference_voices
            .iter_mut()
            .find(|reference| reference.id == target_id)
    {
        reference.audio_path = path_str.clone();
        if reference.label.trim().is_empty() || reference.label.starts_with("Reference ") {
            reference.label = "Reference mic".to_string();
        }
        crate::config::save_config(&app.config);
    }
    state::with_state(|s| {
        s.is_mic_recording = false;
        s.error = None;
        s.status = "Reference mic clip saved".to_string();
    });
    recognize_reference(&target_id);
}

pub(super) fn preview_voice(speaker_name: &str) {
    let (profile, text, speaker) = match crate::APP.lock() {
        Ok(app) => {
            let lang = app.config.ui_language.clone();
            let locale = LocaleText::get(&lang);
            let speaker = if speaker_name.trim().is_empty() {
                describe_voice(app.config.tts_playground.method.clone())
            } else {
                speaker_name.to_string()
            };
            (
                TtsRequestProfile::from(&app.config.tts_playground),
                random_preview_text(&locale, &speaker),
                speaker,
            )
        }
        Err(_) => return,
    };
    TTS_MANAGER.speak_interrupt_with_profile(&text, 0, profile);
    state::with_state(|s| {
        s.status = format!("Previewing {speaker}");
        s.error = None;
    });
    state::sync_to_webview();
}

pub(super) fn reset_provider(provider: &str) {
    if let Ok(mut app) = crate::APP.lock() {
        match provider {
            "edge" => app.config.tts_playground.edge_settings = Default::default(),
            "magpie" => app.config.tts_playground.magpie_settings = Default::default(),
            "kokoro" => app.config.tts_playground.kokoro_settings = Default::default(),
            "supertonic" => app.config.tts_playground.supertonic_settings = Default::default(),
            "vieneu" => app.config.tts_playground.vieneu_settings = Default::default(),
            "stepAudio" => app.config.tts_playground.step_audio_settings = Default::default(),
            _ => {}
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}

pub(super) fn use_current_as_source() {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return;
    };
    let Some(audio) = cache_get(&current.id) else {
        return;
    };
    let path = std::env::temp_dir().join(format!("sgt-tts-source-{}.wav", current.id));
    if let Err(err) = std::fs::write(&path, &audio.wav_data) {
        state::with_state(|s| {
            s.error = Some(format!("Failed to save source WAV: {err}"));
        });
        state::sync_to_webview();
        return;
    }
    if let Ok(mut app) = crate::APP.lock() {
        let s = &mut app.config.tts_playground.step_audio_edit_settings;
        s.source_audio_path = path.display().to_string();
        if s.source_text.trim().is_empty() {
            s.source_text = current.text.clone();
        }
        crate::config::save_config(&app.config);
    }
    state::with_state(|s| {
        s.status = "Source audio set from current clip".to_string();
    });
    state::sync_to_webview();
}

pub(super) fn add_reference() {
    if let Ok(mut app) = crate::APP.lock() {
        let idx = app.config.step_audio_reference_voices.len() + 1;
        let id = format!(
            "ref-{}-{}",
            chrono::Local::now().format("%Y%m%d%H%M%S"),
            idx
        );
        app.config
            .step_audio_reference_voices
            .push(crate::config::StepAudioReferenceVoice::new(
                id,
                format!("Reference {idx}"),
            ));
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}

pub(super) fn update_reference(id: &str, label: Option<&str>, transcript: Option<&str>) {
    if id.trim().is_empty() {
        return;
    }
    if let Ok(mut app) = crate::APP.lock()
        && let Some(reference) = app
            .config
            .step_audio_reference_voices
            .iter_mut()
            .find(|reference| reference.id == id)
    {
        if let Some(label) = label {
            reference.label = label.to_string();
        }
        if let Some(transcript) = transcript {
            reference.transcript = transcript.to_string();
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}

pub(super) fn delete_reference(id: &str) {
    if id.trim().is_empty() {
        return;
    }
    if let Ok(mut app) = crate::APP.lock() {
        app.config
            .step_audio_reference_voices
            .retain(|reference| reference.id != id);
        if app
            .config
            .tts_playground
            .step_audio_settings
            .reference_voice_id
            == id
        {
            app.config
                .tts_playground
                .step_audio_settings
                .reference_voice_id
                .clear();
        }
        if app.config.tts_playground.vieneu_settings.reference_voice_id == id {
            app.config
                .tts_playground
                .vieneu_settings
                .reference_voice_id
                .clear();
        }
        if app.config.step_audio_settings.reference_voice_id == id {
            app.config.step_audio_settings.reference_voice_id.clear();
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}

pub(super) fn pick_reference_audio(id: &str) -> Result<Option<String>, String> {
    let Some(path) = file_dialogs::pick_audio_file_dialog()? else {
        return Ok(None);
    };
    let path_str = path.display().to_string();
    if let Ok(mut app) = crate::APP.lock()
        && let Some(reference) = app
            .config
            .step_audio_reference_voices
            .iter_mut()
            .find(|reference| reference.id == id)
    {
        reference.audio_path = path_str.clone();
        if reference.label.trim().is_empty() || reference.label.starts_with("Reference ") {
            reference.label = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Reference voice")
                .to_string();
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
    recognize_reference(id);
    Ok(Some(path_str))
}

pub(super) fn recognize_reference(id: &str) {
    let reference = match crate::APP.lock() {
        Ok(app) => app
            .config
            .step_audio_reference_voices
            .iter()
            .find(|reference| reference.id == id)
            .cloned(),
        Err(_) => None,
    };
    let Some(reference) = reference else { return };
    if reference.audio_path.trim().is_empty() {
        state::with_state(|s| s.error = Some("Pick reference audio first.".to_string()));
        state::sync_to_webview();
        return;
    }
    let reference_id = reference.id.clone();
    let audio_path = reference.audio_path.clone();
    state::with_state(|s| {
        s.error = None;
        s.status = "Recognizing reference transcript...".to_string();
    });
    state::sync_to_webview();
    std::thread::spawn(move || {
        let preset = reference_transcription_preset();
        let result = std::path::Path::new(&audio_path)
            .canonicalize()
            .map_err(|err| err.to_string())
            .and_then(|path| {
                crate::gui::app::input_handler::load_audio_file(&path)
                    .ok_or_else(|| "Could not decode reference audio.".to_string())
            })
            .and_then(|wav_data| {
                crate::api::audio::execute_audio_processing_logic(&preset, wav_data)
                    .map_err(|err| err.to_string())
            })
            .map(|text| text.trim().to_string());
        match result {
            Ok(transcript) => {
                if let Ok(mut app) = crate::APP.lock()
                    && let Some(reference) = app
                        .config
                        .step_audio_reference_voices
                        .iter_mut()
                        .find(|reference| reference.id == reference_id)
                {
                    reference.transcript = transcript;
                    crate::config::save_config(&app.config);
                }
                state::with_state(|s| {
                    s.error = None;
                    s.status = "Reference transcript recognized.".to_string();
                });
            }
            Err(err) => {
                state::with_state(|s| {
                    s.error = Some(err);
                    s.status.clear();
                });
            }
        }
        state::sync_to_webview();
    });
}

pub(super) fn play_reference(id: &str) {
    let reference = match crate::APP.lock() {
        Ok(app) => app
            .config
            .step_audio_reference_voices
            .iter()
            .find(|reference| reference.id == id)
            .cloned(),
        Err(_) => None,
    };
    let Some(reference) = reference else { return };
    if reference.audio_path.trim().is_empty() {
        state::with_state(|s| s.error = Some("Pick reference audio first.".to_string()));
        state::sync_to_webview();
        return;
    }
    let result = std::path::Path::new(&reference.audio_path)
        .canonicalize()
        .map_err(|err| err.to_string())
        .and_then(|path| {
            crate::gui::app::input_handler::load_audio_file(&path)
                .ok_or_else(|| "Could not decode reference audio.".to_string())
        })
        .and_then(|wav_data| super::library::decode_wav_to_24khz_mono(&wav_data));
    match result {
        Ok(samples) => {
            TTS_MANAGER.play_pcm_interrupt(samples, 0);
            state::with_state(|s| {
                s.error = None;
                s.status = format!("Previewing {}", reference.label);
            });
        }
        Err(err) => state::with_state(|s| s.error = Some(err)),
    }
    state::sync_to_webview();
}

pub(super) fn use_reference(id: &str, target: &str) {
    if let Ok(mut app) = crate::APP.lock() {
        match target {
            "global" => {
                app.config.step_audio_settings.reference_voice_id = id.to_string();
            }
            _ => {
                app.config
                    .tts_playground
                    .step_audio_settings
                    .reference_voice_id = id.to_string();
                app.config.tts_playground.method = TtsMethod::StepAudioEditX;
                app.config.tts_playground.mode = crate::config::TtsPlaygroundMode::TtsClone;
            }
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}
