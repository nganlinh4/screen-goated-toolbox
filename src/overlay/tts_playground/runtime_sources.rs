use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::api::tts::types::TtsRequestProfile;
use crate::gui::locale::LocaleText;

use super::file_dialogs;
use super::runtime::{MIC_SESSION, MicSession, describe_voice, random_preview_text};
use super::state;

pub(super) fn pick_source_audio() -> Result<Option<String>, String> {
    match file_dialogs::pick_audio_file_dialog()? {
        Some(path) => {
            let path_str = path.display().to_string();
            if let Ok(mut app) = crate::APP.lock() {
                let previous = app
                    .config
                    .tts_playground
                    .source_audio
                    .source_audio_path
                    .clone();
                app.config.tts_playground.source_audio.source_audio_path = path_str.clone();
                crate::config::save_config(&app.config);
                super::library::delete_replaced_managed_audio(&previous, &path);
            }
            state::sync_to_webview();
            Ok(Some(path_str))
        }
        None => Ok(None),
    }
}

pub(super) fn start_mic_recording() {
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
        let previous = app
            .config
            .tts_playground
            .source_audio
            .source_audio_path
            .clone();
        app.config.tts_playground.source_audio.source_audio_path = path.display().to_string();
        crate::config::save_config(&app.config);
        super::library::delete_replaced_managed_audio(&previous, &path);
    }
    let duration_sec = samples.len() as f32 / 16_000.0;
    state::with_state(|s| {
        s.is_mic_recording = false;
        s.error = None;
        s.status = format!("Mic clip saved ({duration_sec:.1}s) as the source audio");
    });
    state::sync_to_webview();
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
    super::runtime_playback::preview_profile(&text, profile);
    state::with_state(|s| {
        s.status = format!("Previewing {speaker}");
        s.error = None;
    });
    state::sync_to_webview();
}

pub(super) fn reset_provider(provider: &str) {
    if let Ok(mut app) = crate::APP.lock() {
        if provider == "edge" {
            app.config.tts_playground.edge_settings = Default::default();
        }
        crate::config::save_config(&app.config);
    }
    state::sync_to_webview();
}
