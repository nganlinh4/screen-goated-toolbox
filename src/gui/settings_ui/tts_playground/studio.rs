use super::export;
use super::state::{ArtifactResult, ExportResult, TtsPlaygroundArtifact, TtsPlaygroundUiState};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::{Config, TtsMethod};
use crate::gui::locale::LocaleText;
use eframe::egui;
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
    ui.label(egui::RichText::new(text.tts_playground_text_label).strong());
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut config.tts_playground.draft_text)
                .desired_rows(11)
                .desired_width(f32::INFINITY)
                .hint_text(text.tts_playground_text_hint),
        )
        .changed();
    ui.label(text.char_count_fmt.replace(
        "{}",
        &config.tts_playground.draft_text.chars().count().to_string(),
    ));

    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !state.is_generating && !config.tts_playground.draft_text.trim().is_empty(),
                egui::Button::new(text.tts_playground_generate),
            )
            .clicked()
        {
            start_generation(config, state);
        }
        if ui.button(text.tts_playground_clear).clicked() {
            config.tts_playground.draft_text.clear();
            changed = true;
        }
    });

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

pub(super) fn stop_player(state: &mut TtsPlaygroundUiState) {
    TTS_MANAGER.stop();
    state.player.reset();
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
            .synthesize_to_wav_with_profile(&text, profile)
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
        TtsMethod::VoxtralTts => {
            let v = config.tts_playground.voxtral_settings.voice.trim();
            if v.is_empty() {
                "Mistral Voxtral 4B TTS".to_string()
            } else {
                format!("Mistral Voxtral 4B TTS · {v}")
            }
        }
    }
}

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
