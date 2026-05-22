use super::export;
use super::state::{ArtifactResult, ExportResult, TtsPlaygroundArtifact, TtsPlaygroundUiState};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::{Config, TtsMethod, TtsPlaygroundMode, step_audio_tts_text_issue};
use crate::gui::locale::LocaleText;
use eframe::egui;
use egui::text::CCursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(super) fn render_studio_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    config: &mut Config,
    text: &LocaleText,
    state: &mut TtsPlaygroundUiState,
) -> bool {
    poll_generation(state);
    poll_export(state);
    update_finished_playback(state);

    let mut changed = false;
    match config.tts_playground.mode {
        TtsPlaygroundMode::TtsClone | TtsPlaygroundMode::ReferenceLibrary => {
            ui.label(egui::RichText::new(text.tts_playground_text_label).strong());
            let text_edit_output = egui::TextEdit::multiline(&mut config.tts_playground.draft_text)
                .desired_rows(11)
                .desired_width(f32::INFINITY)
                .hint_text(text.tts_playground_text_hint)
                .show(ui);
            let text_edit_state = text_edit_output.state;
            let cursor_range = text_edit_output
                .cursor_range
                .or_else(|| text_edit_state.cursor.char_range());
            if let Some(cursor_range) = cursor_range {
                state.draft_text_cursor_range = Some(cursor_range);
            }
            changed |= text_edit_output.response.changed();
            ui.label(text.char_count_fmt.replace(
                "{}",
                &config.tts_playground.draft_text.chars().count().to_string(),
            ));

            ui.horizontal_wrapped(|ui| {
                let step_audio_issue = current_step_audio_issue(config);
                if let Some(issue) = step_audio_issue {
                    ui.colored_label(egui::Color32::from_rgb(210, 80, 80), issue);
                }
                let can_generate = !state.is_generating
                    && !config.tts_playground.draft_text.trim().is_empty()
                    && step_audio_issue.is_none();
                if ui
                    .add_enabled(
                        can_generate,
                        primary_generate_button(text.tts_playground_generate),
                    )
                    .clicked()
                {
                    start_generation(config, state);
                }
                if ui.button(text.tts_playground_clear).clicked() {
                    config.tts_playground.draft_text.clear();
                    state.draft_text_cursor_range =
                        Some(egui::text::CCursorRange::one(CCursor::new(0)));
                    changed = true;
                }
            });
        }
        TtsPlaygroundMode::AudioEdit => {
            ui.label(egui::RichText::new(text.tts_step_audio_edit_output).strong());
            ui.add_space(8.0);
            let edit = &config.tts_playground.step_audio_edit_settings;
            let can_generate = !state.is_generating
                && !edit.source_audio_path.trim().is_empty()
                && !edit.source_text.trim().is_empty()
                && (edit.edit_type != "paralinguistic" || !edit.target_text.trim().is_empty());
            if ui
                .add_enabled(
                    can_generate,
                    primary_generate_button(text.tts_step_audio_generate_edit),
                )
                .clicked()
            {
                start_step_audio_edit(config, state);
            }
        }
        TtsPlaygroundMode::SpeechToSpeech => {
            let source = config
                .tts_playground
                .step_audio_edit_settings
                .source_audio_path
                .trim();
            ui.label(egui::RichText::new("Speech-to-speech").strong());
            let can_generate = !state.is_generating && !source.is_empty();
            if ui
                .add_enabled(
                    can_generate,
                    primary_generate_button(text.tts_playground_generate),
                )
                .clicked()
            {
                start_s2s_generation(config, state);
            }
        }
    }

    ui.separator();
    render_player(ui, ctx, text, state);
    ui.separator();
    render_exports(ui, text, state);
    render_recent(ui, text, state);

    if state.is_generating || state.is_exporting || state.player.playing {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    changed
}

fn primary_generate_button(label: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(label)
        .fill(egui::Color32::from_rgb(30, 112, 210))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(104, 169, 255),
        ))
        .min_size(egui::vec2(138.0, 32.0))
}

fn current_step_audio_issue(config: &Config) -> Option<&'static str> {
    if matches!(
        config.tts_playground.mode,
        TtsPlaygroundMode::ReferenceLibrary
    ) || (matches!(config.tts_playground.mode, TtsPlaygroundMode::TtsClone)
        && matches!(config.tts_playground.method, TtsMethod::StepAudioEditX))
    {
        step_audio_tts_text_issue(&config.tts_playground.draft_text)
    } else {
        None
    }
}

fn start_step_audio_edit(config: &Config, state: &mut TtsPlaygroundUiState) {
    let edit = config.tts_playground.step_audio_edit_settings.clone();
    if edit.source_audio_path.trim().is_empty() || edit.source_text.trim().is_empty() {
        return;
    }
    TTS_MANAGER.stop();
    state.player.reset();
    state.is_generating = true;
    let cancel = Arc::new(AtomicBool::new(false));
    state.generation_cancel = Some(cancel.clone());
    state.status.clear();
    state.error = None;
    let voice_label = format!(
        "Step Audio EditX {}{}",
        edit.edit_type,
        if edit.edit_info.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", edit.edit_info)
        }
    );
    let (tx, rx) = mpsc::channel();
    state.job_rx = Some(rx);
    std::thread::spawn(move || {
        let started = Instant::now();
        let result = crate::api::tts::worker::synthesize_step_audio_edit_to_wav_cancel(
            edit.source_audio_path.clone(),
            edit.source_text.clone(),
            edit.edit_type.clone(),
            edit.edit_info.clone(),
            edit.target_text.clone(),
            cancel,
        )
        .map(|audio| {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            TtsPlaygroundArtifact {
                id,
                text: if edit.edit_type == "paralinguistic" {
                    edit.target_text
                } else {
                    edit.source_text
                },
                method: TtsMethod::StepAudioEditX,
                voice_label,
                pcm_samples: audio.pcm_samples,
                wav_data: audio.wav_data,
                sample_rate: audio.sample_rate,
                duration_ms: audio.duration_ms,
                latency_ms: started.elapsed().as_millis(),
                created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
            }
        })
        .map_err(|err| err.to_string());
        let _ = tx.send(result);
    });
}

fn start_s2s_generation(config: &Config, state: &mut TtsPlaygroundUiState) {
    let source_path = config
        .tts_playground
        .step_audio_edit_settings
        .source_audio_path
        .trim()
        .to_string();
    if source_path.is_empty() {
        state.error = Some("Pick or record source audio first.".to_string());
        return;
    }
    let settings = match crate::api::realtime_audio::s2s::default_batch_settings_for_target(
        &config.realtime_target_language,
        &config.tts_playground.gemini_model,
        &config.tts_playground.gemini_voice,
        &config.tts_playground.gemini_speed,
    ) {
        Ok(settings) => settings,
        Err(error) => {
            state.error = Some(error.to_string());
            return;
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = cancel.clone();
    let voice_label = config.tts_playground.gemini_voice.clone();
    let (tx, rx) = mpsc::channel::<ArtifactResult>();
    state.job_rx = Some(rx);
    state.generation_cancel = Some(cancel);
    state.is_generating = true;
    state.status = "Generating Gemini S2S audio...".to_string();
    state.error = None;
    std::thread::spawn(move || {
        let started = Instant::now();
        let result = decode_audio_16k_mono(&source_path)
            .and_then(|samples| {
                crate::api::realtime_audio::s2s::run_gemini_live_s2s_batch(
                    samples,
                    settings,
                    thread_cancel,
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
                let id = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                Ok(TtsPlaygroundArtifact {
                    id,
                    text: format!(
                        "Source: {}\nTarget: {}",
                        source_text.join(" "),
                        target_text.join(" ")
                    ),
                    method: TtsMethod::GeminiLive,
                    voice_label,
                    pcm_samples,
                    wav_data,
                    sample_rate: 24_000,
                    duration_ms,
                    latency_ms: started.elapsed().as_millis(),
                    created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
                })
            });
        let _ = tx.send(result);
    });
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

pub(super) fn stop_player(state: &mut TtsPlaygroundUiState) {
    TTS_MANAGER.stop();
    state.player.reset();
}

fn cancel_generation(state: &mut TtsPlaygroundUiState) {
    if let Some(cancel) = &state.generation_cancel {
        cancel.store(true, Ordering::SeqCst);
    }
    TTS_MANAGER.stop();
    state.job_rx = None;
    state.generation_cancel = None;
    state.is_generating = false;
    state.player.reset();
    state.error = None;
    state.status = "Generation cancelled".to_string();
}

fn poll_generation(state: &mut TtsPlaygroundUiState) {
    let mut completed: Option<ArtifactResult> = None;
    if let Some(rx) = &state.job_rx {
        match rx.try_recv() {
            Ok(result) => completed = Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                completed = Some(Err("TTS generation worker disconnected".to_string()));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    if let Some(result) = completed {
        state.job_rx = None;
        state.generation_cancel = None;
        state.is_generating = false;
        match result {
            Ok(artifact) => {
                state.status = format!(
                    "{} | {:.1}s | {} | {}ms",
                    artifact.voice_label,
                    artifact.duration_sec(),
                    artifact.size_label(),
                    artifact.latency_ms
                );
                state.error = None;
                state.player.reset();
                if let Ok(app) = crate::APP.lock() {
                    app.history
                        .save_audio(artifact.wav_data.clone(), artifact.text.clone());
                }
                state.current = Some(artifact.clone());
                state.push_recent(artifact);
            }
            Err(error) if error == "Generation cancelled" => {
                state.error = None;
                state.status = error;
            }
            Err(error) => {
                state.error = Some(error);
                state.status.clear();
            }
        }
    }
}

fn poll_export(state: &mut TtsPlaygroundUiState) {
    let mut completed: Option<ExportResult> = None;
    if let Some(rx) = &state.export_rx {
        match rx.try_recv() {
            Ok(result) => completed = Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                completed = Some(Err("MP3 export worker disconnected".to_string()));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    if let Some(result) = completed {
        state.export_rx = None;
        state.is_exporting = false;
        match result {
            Ok(path) => {
                state.status = format!("Saved {path}");
                state.error = None;
            }
            Err(error) if error != "Save cancelled" => state.error = Some(error),
            Err(_) => {}
        }
    }
}

fn start_generation(config: &Config, state: &mut TtsPlaygroundUiState) {
    let text = config.tts_playground.draft_text.trim().to_string();
    if text.is_empty() {
        return;
    }
    TTS_MANAGER.stop();
    state.player.reset();
    state.is_generating = true;
    let cancel = Arc::new(AtomicBool::new(false));
    state.generation_cancel = Some(cancel.clone());
    state.status.clear();
    state.error = None;

    let profile = TtsRequestProfile::from(&config.tts_playground);
    let method = config.tts_playground.method.clone();
    let voice_label = voice_label(config);
    let (tx, rx) = mpsc::channel();
    state.job_rx = Some(rx);

    std::thread::spawn(move || {
        let started = Instant::now();
        let result = TTS_MANAGER
            .synthesize_to_wav_with_profile_cancel(&text, profile, cancel)
            .map(|audio| {
                let id = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                TtsPlaygroundArtifact {
                    id,
                    text,
                    method,
                    voice_label,
                    pcm_samples: audio.pcm_samples,
                    wav_data: audio.wav_data,
                    sample_rate: audio.sample_rate,
                    duration_ms: audio.duration_ms,
                    latency_ms: started.elapsed().as_millis(),
                    created_label: chrono::Local::now().format("%H:%M:%S").to_string(),
                }
            })
            .map_err(|err| err.to_string());
        let _ = tx.send(result);
    });
}

fn render_player(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    text: &LocaleText,
    state: &mut TtsPlaygroundUiState,
) {
    if state.is_generating {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text.tts_playground_generating);
            if ui.button(text.cancel_label).clicked() {
                cancel_generation(state);
            }
        });
    }
    if state.is_exporting {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text.tts_playground_exporting_mp3);
        });
    }

    if let Some(error) = &state.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    } else if !state.status.is_empty() {
        ui.label(&state.status);
    } else if state.current.is_none() {
        ui.label(text.tts_playground_no_audio);
    }

    let Some(artifact) = state.current.clone() else {
        return;
    };

    let current_sample = state
        .player
        .current_sample(artifact.sample_rate, artifact.pcm_samples.len());
    let mut position = current_sample as f32 / artifact.sample_rate as f32;
    let duration = artifact.duration_sec().max(0.001);

    let seek_response = ui.add(
        egui::Slider::new(&mut position, 0.0..=duration)
            .show_value(false)
            .text(""),
    );
    if seek_response.changed() {
        let sample = artifact.sample_for_sec(position);
        if state.player.playing {
            play_artifact(state, &artifact, sample);
        } else {
            state.player.paused_sample = sample;
        }
    }

    ui.horizontal(|ui| {
        ui.label(format_time(position));
        ui.label("/");
        ui.label(format_time(duration));
    });

    ui.horizontal_wrapped(|ui| {
        let play_label = if state.player.playing {
            text.tts_playground_pause
        } else if state.player.paused_sample > 0 {
            text.tts_playground_resume
        } else {
            text.tts_playground_play
        };
        if ui.button(play_label).clicked() {
            if state.player.playing {
                pause_artifact(state, &artifact);
            } else {
                play_artifact(state, &artifact, state.player.paused_sample);
            }
        }
        if ui.button(text.tts_playground_stop).clicked() {
            stop_player(state);
        }
        if ui.button(text.tts_playground_replay).clicked() {
            play_artifact(state, &artifact, 0);
        }
    });

    if seek_response.hovered() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn render_exports(ui: &mut egui::Ui, text: &LocaleText, state: &mut TtsPlaygroundUiState) {
    let Some(artifact) = state.current.clone() else {
        return;
    };

    ui.horizontal_wrapped(|ui| {
        if ui.button(text.tts_playground_download_wav).clicked() {
            match export::save_wav_dialog(&artifact) {
                Ok(path) => state.status = format!("Saved {}", path.display()),
                Err(error) if error != "Save cancelled" => state.error = Some(error),
                Err(_) => {}
            }
        }
        if ui
            .add_enabled(
                !state.is_exporting,
                egui::Button::new(text.tts_playground_download_mp3),
            )
            .clicked()
        {
            start_mp3_export(artifact, state);
        }
    });
}

fn start_mp3_export(artifact: TtsPlaygroundArtifact, state: &mut TtsPlaygroundUiState) {
    let (tx, rx) = mpsc::channel();
    state.export_rx = Some(rx);
    state.is_exporting = true;
    state.error = None;
    std::thread::spawn(move || {
        let result = export::save_mp3_dialog(&artifact).map(|path| path.display().to_string());
        let _ = tx.send(result);
    });
}

fn render_recent(ui: &mut egui::Ui, text: &LocaleText, state: &mut TtsPlaygroundUiState) {
    if state.recent.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text.tts_playground_recent).strong());
    let mut selected: Option<TtsPlaygroundArtifact> = None;
    let mut delete_id: Option<u64> = None;
    egui::ScrollArea::vertical()
        .max_height(90.0)
        .show(ui, |ui| {
            for artifact in &state.recent {
                let preview = artifact.text.chars().take(38).collect::<String>();
                let label = format!(
                    "{} | {} | {:.1}s | {}",
                    artifact.created_label,
                    artifact.voice_label,
                    artifact.duration_sec(),
                    preview
                );
                let row_width = ui.available_width();
                let row_height = 18.0;
                let (row_rect, row_response) =
                    ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
                let delete_rect = egui::Rect::from_center_size(
                    egui::pos2(row_rect.right() - 9.0, row_rect.center().y),
                    egui::vec2(18.0, 18.0),
                );
                let label_rect = egui::Rect::from_min_max(
                    row_rect.left_center() - egui::vec2(0.0, row_height / 2.0),
                    egui::pos2(delete_rect.left() - 4.0, row_rect.bottom()),
                );

                if row_response.hovered() {
                    ui.painter().rect_filled(
                        row_rect,
                        3.0,
                        ui.visuals().widgets.hovered.bg_fill.linear_multiply(0.35),
                    );
                }
                ui.painter().with_clip_rect(label_rect).text(
                    label_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                );

                let delete_id_source = ui.id().with(("tts_recent_delete", artifact.id));
                let delete_response =
                    ui.interact(delete_rect, delete_id_source, egui::Sense::click());
                let icon_color = if delete_response.hovered() {
                    ui.visuals().widgets.hovered.fg_stroke.color
                } else {
                    ui.visuals().widgets.inactive.fg_stroke.color
                };
                crate::gui::icons::paint_icon(
                    ui.painter(),
                    delete_rect,
                    crate::gui::icons::Icon::DeleteLarge,
                    icon_color,
                );

                if delete_response
                    .on_hover_text(text.history_delete_tooltip)
                    .clicked()
                {
                    delete_id = Some(artifact.id);
                } else if row_response.clicked() {
                    selected = Some(artifact.clone());
                }
            }
        });
    if let Some(id) = delete_id {
        stop_player(state);
        state.delete_recent(id);
        return;
    }
    if let Some(artifact) = selected {
        stop_player(state);
        state.current = Some(artifact);
    }
}

fn play_artifact(
    state: &mut TtsPlaygroundUiState,
    artifact: &TtsPlaygroundArtifact,
    sample: usize,
) {
    let start_sample = if sample >= artifact.pcm_samples.len() {
        0
    } else {
        sample
    };
    eprintln!(
        "[TTS Playground] ui-play artifact={} start_sample={} total_samples={} was_playing={}",
        artifact.id,
        start_sample,
        artifact.pcm_samples.len(),
        state.player.playing
    );
    TTS_MANAGER.play_pcm_interrupt(artifact.pcm_samples.clone(), start_sample);
    state.player.playing = true;
    state.player.start_sample = start_sample;
    state.player.paused_sample = start_sample;
    state.player.started_at = Some(Instant::now());
}

fn pause_artifact(state: &mut TtsPlaygroundUiState, artifact: &TtsPlaygroundArtifact) {
    let sample = state
        .player
        .current_sample(artifact.sample_rate, artifact.pcm_samples.len());
    TTS_MANAGER.stop();
    eprintln!(
        "[TTS Playground] ui-pause artifact={} paused_sample={} total_samples={}",
        artifact.id,
        sample,
        artifact.pcm_samples.len()
    );
    state.player.playing = false;
    state.player.paused_sample = sample;
    state.player.started_at = None;
}

fn update_finished_playback(state: &mut TtsPlaygroundUiState) {
    if let Some(artifact) = &state.current
        && state.player.playing
        && state
            .player
            .current_sample(artifact.sample_rate, artifact.pcm_samples.len())
            >= artifact.pcm_samples.len()
    {
        eprintln!("[TTS Playground] ui-play-complete artifact={}", artifact.id);
        state.player.reset();
    }
}

fn voice_label(config: &Config) -> String {
    match config.tts_playground.method {
        TtsMethod::GeminiLive => config.tts_playground.gemini_voice.clone(),
        TtsMethod::GoogleTranslate => "Google Translate".to_string(),
        TtsMethod::EdgeTTS => config.tts_playground.edge_voice.clone(),
        TtsMethod::FishAudioS2Pro => "Removed TTS model".to_string(),
        TtsMethod::StepAudioEditX => {
            let v = config.tts_playground.step_audio_settings.voice.trim();
            if v.is_empty() {
                "Step Audio EditX".to_string()
            } else {
                format!("Step · {v}")
            }
        }
        TtsMethod::MagpieMultilingual => {
            let v = config.tts_playground.magpie_settings.voice.trim();
            if v.is_empty() {
                "NVIDIA Magpie-Multilingual 357M".to_string()
            } else {
                format!("NVIDIA Magpie-Multilingual 357M · {v}")
            }
        }
        TtsMethod::Kokoro => {
            let v = config.tts_playground.kokoro_settings.voice.trim();
            if v.is_empty() {
                "Kokoro 82M v1.0".to_string()
            } else {
                format!("Kokoro 82M v1.0 · {v}")
            }
        }
        TtsMethod::Supertonic => {
            let s = &config.tts_playground.supertonic_settings;
            let voice = s
                .voice_configs
                .first()
                .map(|config| config.voice_id.as_str())
                .unwrap_or("M1");
            format!("Supertonic 3 · {voice}")
        }
        TtsMethod::VieneuTts => "VieNeu-TTS-v2 Turbo GPU".to_string(),
        TtsMethod::VoxtralTts => "Removed TTS provider".to_string(),
    }
}

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
