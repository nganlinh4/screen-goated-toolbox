use super::export;
use super::library;
use super::state::{
    MicRecordingState, ReferencePreviewState, TranscriptResult, TtsPlaygroundUiState,
};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::SOURCE_SAMPLE_RATE;
use crate::config::{Config, Preset, ProcessingBlock, StepAudioReferenceVoice, StepAudioSettings};
use eframe::egui;
use std::sync::mpsc;
use std::time::Instant;

pub(super) const TAG_EXAMPLE_TEXT: &str = "[Cantonese] Hôm nay mình thử giọng tham chiếu mới. [laugh] Đoạn này cần tự nhiên hơn, [sigh] nhưng vẫn rõ chữ.";

pub(super) const STEP_AUDIO_PARALINGUISTIC_TAGS: &[&str] = &[
    "[sigh]",
    "[inhale]",
    "[laugh]",
    "[chuckle]",
    "[exhale]",
    "[clears throat]",
    "[snort]",
    "[giggle]",
    "[cough]",
    "[breath]",
    "[uhm]",
    "[Confirmation-en]",
    "[Surprise-oh]",
    "[Surprise-ah]",
    "[Surprise-wa]",
    "[Surprise-yo]",
    "[Dissatisfaction-hnn]",
    "[Question-ei]",
    "[Question-ah]",
    "[Question-en]",
    "[Question-yi]",
    "[Question-oh]",
];

pub(super) fn poll_reference_transcript(
    config: &mut Config,
    state: &mut TtsPlaygroundUiState,
) -> bool {
    let Some(rx) = &state.reference_transcript_rx else {
        return false;
    };
    match rx.try_recv() {
        Ok(Ok((reference_id, transcript))) => {
            let mut changed = false;
            if let Some(reference) = config
                .step_audio_reference_voices
                .iter_mut()
                .find(|item| item.id == reference_id)
            {
                if reference.transcript != transcript {
                    reference.transcript = transcript;
                    changed = true;
                }
            }
            state.reference_transcript_rx = None;
            state.is_recognizing_reference = false;
            state.status = "Reference transcript recognized.".to_string();
            changed
        }
        Ok(Err(err)) => {
            state.reference_transcript_rx = None;
            state.is_recognizing_reference = false;
            state.error = Some(err);
            false
        }
        Err(mpsc::TryRecvError::Empty) => false,
        Err(mpsc::TryRecvError::Disconnected) => {
            state.reference_transcript_rx = None;
            state.is_recognizing_reference = false;
            state.error = Some("Reference transcription worker stopped.".to_string());
            false
        }
    }
}

pub(super) fn render_reference_voice_selector(
    ui: &mut egui::Ui,
    references: &[StepAudioReferenceVoice],
    settings: &mut StepAudioSettings,
    id: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Reference voice:");
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected_reference_label(references, settings))
            .width(220.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut settings.reference_voice_id,
                        String::new(),
                        "Bundled default reference",
                    )
                    .changed();
                for reference in references {
                    changed |= ui
                        .selectable_value(
                            &mut settings.reference_voice_id,
                            reference.id.clone(),
                            reference_label(reference),
                        )
                        .changed();
                }
            });
    });
    changed
}

pub(super) fn render_reference_library(
    ui: &mut egui::Ui,
    config: &mut Config,
    state: &mut TtsPlaygroundUiState,
) -> bool {
    let mut changed = poll_reference_transcript(config, state);
    update_reference_preview(state);
    if state.reference_preview.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    ui.label(egui::RichText::new("Reference voice library").strong());
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Shared by Step Audio TTS, global TTS config, and narration.")
                .small()
                .color(egui::Color32::from_rgb(96, 125, 139)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+ Add reference").clicked() {
                let idx = config.step_audio_reference_voices.len() + 1;
                let id = format!(
                    "ref-{}",
                    chrono::Local::now()
                        .timestamp_nanos_opt()
                        .unwrap_or_default()
                        .unsigned_abs()
                );
                config
                    .step_audio_reference_voices
                    .push(StepAudioReferenceVoice::new(id, format!("Reference {idx}")));
                changed = true;
            }
        });
    });
    ui.add_space(6.0);

    let mut remove_idx = None;
    for idx in 0..config.step_audio_reference_voices.len() {
        changed |= render_reference_row(ui, config, state, idx, &mut remove_idx);
        ui.add_space(6.0);
    }
    if let Some(idx) = remove_idx {
        let removed_id = config.step_audio_reference_voices[idx].id.clone();
        if state
            .reference_preview
            .as_ref()
            .is_some_and(|preview| preview.reference_id == removed_id)
        {
            stop_reference_preview(state);
        }
        config.step_audio_reference_voices.remove(idx);
        if config.step_audio_settings.reference_voice_id == removed_id {
            config.step_audio_settings.reference_voice_id.clear();
        }
        if config.tts_playground.step_audio_settings.reference_voice_id == removed_id {
            config
                .tts_playground
                .step_audio_settings
                .reference_voice_id
                .clear();
        }
        changed = true;
    }

    changed
}

fn render_reference_row(
    ui: &mut egui::Ui,
    config: &mut Config,
    state: &mut TtsPlaygroundUiState,
    idx: usize,
    remove_idx: &mut Option<usize>,
) -> bool {
    let mut changed = false;
    let reference = &mut config.step_audio_reference_voices[idx];
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Label");
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut reference.label)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
                if ui.button("Play").clicked() {
                    preview_reference_voice(reference, state);
                }
                let is_previewing = state
                    .reference_preview
                    .as_ref()
                    .is_some_and(|preview| preview.reference_id == reference.id);
                if ui
                    .add_enabled(is_previewing, egui::Button::new("Stop"))
                    .clicked()
                {
                    stop_reference_preview(state);
                }
                if ui.button("Remove").clicked() {
                    *remove_idx = Some(idx);
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Pick audio").clicked() {
                    match export::pick_audio_file_dialog() {
                        Ok(Some(path)) => {
                            reference.audio_path = path.display().to_string();
                            if reference.label.trim().is_empty()
                                || reference.label.starts_with("Reference ")
                            {
                                reference.label = path
                                    .file_stem()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("Reference voice")
                                    .to_string();
                            }
                            if reference.transcript.trim().is_empty() {
                                start_reference_transcription(
                                    reference.id.clone(),
                                    reference.audio_path.clone(),
                                    state,
                                );
                            }
                            changed = true;
                        }
                        Ok(None) => {}
                        Err(err) => state.error = Some(err),
                    }
                }
                if state.mic_recording.is_some() {
                    if ui.button("Stop mic").clicked() {
                        if let Some(recording) = state.mic_recording.take() {
                            recording
                                .stop
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                            drop(recording.stream);
                            if let Ok(samples) = recording.samples.lock()
                                && let Ok(path) =
                                    library::encode_managed_wav("reference-mic", &samples, 16_000)
                            {
                                reference.audio_path = path.display().to_string();
                                if reference.label.trim().is_empty()
                                    || reference.label.starts_with("Reference ")
                                {
                                    reference.label = "Mic reference".to_string();
                                }
                                if reference.transcript.trim().is_empty() {
                                    start_reference_transcription(
                                        reference.id.clone(),
                                        reference.audio_path.clone(),
                                        state,
                                    );
                                }
                                changed = true;
                            }
                        }
                    }
                } else if ui.button("Record mic").clicked() {
                    let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
                    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let pause = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    match crate::api::realtime_audio::start_mic_capture(
                        samples.clone(),
                        stop.clone(),
                        pause,
                    ) {
                        Ok(stream) => {
                            state.mic_recording = Some(MicRecordingState {
                                samples,
                                stop,
                                stream,
                            });
                        }
                        Err(err) => state.error = Some(err.to_string()),
                    }
                }
                if ui
                    .add_enabled(
                        !state.is_recognizing_reference && !reference.audio_path.trim().is_empty(),
                        egui::Button::new("Auto recognize"),
                    )
                    .clicked()
                {
                    start_reference_transcription(
                        reference.id.clone(),
                        reference.audio_path.clone(),
                        state,
                    );
                }
                if ui.button("Use in playground").clicked() {
                    config.tts_playground.step_audio_settings.reference_voice_id =
                        reference.id.clone();
                    config.tts_playground.method = crate::config::TtsMethod::StepAudioEditX;
                    changed = true;
                }
                if ui.button("Use globally").clicked() {
                    config.step_audio_settings.reference_voice_id = reference.id.clone();
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                let path_text = if reference.audio_path.trim().is_empty() {
                    "No reference audio selected"
                } else {
                    reference.audio_path.as_str()
                };
                ui.label(
                    egui::RichText::new(path_text)
                        .small()
                        .color(egui::Color32::from_rgb(96, 125, 139)),
                );
                if let Some(preview) = &state.reference_preview
                    && preview.reference_id == reference.id
                {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {}",
                            format_seconds(preview.started_at.elapsed().as_secs_f32()),
                            format_seconds(preview.samples_len as f32 / SOURCE_SAMPLE_RATE as f32)
                        ))
                        .small()
                        .color(egui::Color32::from_rgb(30, 112, 210)),
                    );
                }
            });
            ui.label(egui::RichText::new("Exact reference transcript").small());
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut reference.transcript)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .changed();
        });
    changed
}

fn preview_reference_voice(reference: &StepAudioReferenceVoice, state: &mut TtsPlaygroundUiState) {
    if reference.audio_path.trim().is_empty() {
        state.error = Some("Pick or record reference audio before previewing.".to_string());
        return;
    }
    let result = std::path::Path::new(&reference.audio_path)
        .canonicalize()
        .map_err(|err| err.to_string())
        .and_then(|path| {
            crate::gui::app::input_handler::load_audio_file(&path)
                .ok_or_else(|| "Could not decode reference audio.".to_string())
        })
        .and_then(|wav_data| library::decode_wav_to_24khz_mono(&wav_data));

    match result {
        Ok(samples) if samples.is_empty() => {
            state.error = Some("Reference audio has no samples.".to_string());
        }
        Ok(samples) => {
            TTS_MANAGER.play_pcm_interrupt(samples.clone(), 0);
            state.reference_preview = Some(ReferencePreviewState {
                reference_id: reference.id.clone(),
                started_at: Instant::now(),
                samples_len: samples.len(),
            });
            state.error = None;
            state.status.clear();
        }
        Err(err) => state.error = Some(err),
    }
}

fn stop_reference_preview(state: &mut TtsPlaygroundUiState) {
    TTS_MANAGER.stop();
    state.reference_preview = None;
}

fn update_reference_preview(state: &mut TtsPlaygroundUiState) {
    let Some(preview) = &state.reference_preview else {
        return;
    };
    let duration = preview.samples_len as f32 / SOURCE_SAMPLE_RATE as f32;
    if preview.started_at.elapsed().as_secs_f32() >= duration {
        state.reference_preview = None;
    }
}

fn format_seconds(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn start_reference_transcription(
    reference_id: String,
    audio_path: String,
    state: &mut TtsPlaygroundUiState,
) {
    let preset = reference_transcription_preset();
    let (tx, rx) = mpsc::channel::<TranscriptResult>();
    state.reference_transcript_rx = Some(rx);
    state.is_recognizing_reference = true;
    state.error = None;
    state.status = "Recognizing reference transcript...".to_string();
    std::thread::spawn(move || {
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
            .map(|text| (reference_id, text.trim().to_string()));
        let _ = tx.send(result);
    });
}

fn reference_transcription_preset() -> Preset {
    if let Ok(app) = crate::APP.lock() {
        if let Some(preset) = app
            .config
            .presets
            .get(app.config.active_preset_idx)
            .filter(|preset| preset.blocks.iter().any(is_audio_processing_block))
            .cloned()
        {
            return preset;
        }
        if let Some(preset) = app
            .config
            .presets
            .iter()
            .find(|preset| preset.blocks.iter().any(is_audio_processing_block))
            .cloned()
        {
            return preset;
        }
    }
    fallback_whisper_reference_preset()
}

fn is_audio_processing_block(block: &ProcessingBlock) -> bool {
    block.block_type == "audio"
}

fn fallback_whisper_reference_preset() -> Preset {
    Preset {
        id: "step_audio_reference_transcribe".to_string(),
        name: "Step Audio reference transcript".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "audio".to_string(),
            model: crate::model_config::PRESET_AUDIO_TRANSCRIBE_MODEL_ID.to_string(),
            prompt: "Transcribe the audio exactly. Output ONLY the transcript.".to_string(),
            selected_language: "Auto".to_string(),
            show_overlay: false,
            streaming_enabled: false,
            ..ProcessingBlock::default()
        }],
        preset_type: "audio".to_string(),
        audio_source: "mic".to_string(),
        audio_processing_mode: "record_then_process".to_string(),
        ..Preset::default()
    }
}

fn selected_reference_label(
    references: &[StepAudioReferenceVoice],
    settings: &StepAudioSettings,
) -> String {
    references
        .iter()
        .find(|item| item.id == settings.reference_voice_id)
        .map(reference_label)
        .unwrap_or_else(|| "Bundled default reference".to_string())
}

pub(super) fn reference_label(reference: &StepAudioReferenceVoice) -> String {
    if reference.label.trim().is_empty() {
        "Untitled reference".to_string()
    } else {
        reference.label.clone()
    }
}
