//! UI state for the WRY TTS Playground window.
//!
//! Builds the JSON view model from persisted config, locale labels, runtime
//! playback state, recent clips, and provider catalogs.

use std::sync::Mutex;

use serde::Serialize;

use crate::config::Config;
use crate::gui::locale::LocaleText;

use super::catalogs::CatalogsView;

lazy_static::lazy_static! {
    pub(super) static ref UI_STATE: Mutex<UiState> = Mutex::new(UiState::default());
}

#[derive(Default, Clone)]
pub(super) struct UiState {
    pub(super) is_generating: bool,
    pub(super) is_exporting: bool,
    pub(super) is_mic_recording: bool,
    pub(super) is_playing: bool,
    pub(super) paused: bool,
    pub(super) position_sec: f32,
    pub(super) status: String,
    pub(super) error: Option<String>,
    pub(super) current: Option<CurrentClip>,
    pub(super) recent: Vec<RecentClip>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CurrentClip {
    pub id: String,
    pub text: String,
    pub voice_label: String,
    pub created_label: String,
    pub duration_sec: f32,
    pub sample_rate: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecentClip {
    pub id: String,
    pub text: String,
    pub voice_label: String,
    pub created_label: String,
    pub duration_sec: f32,
}

pub(super) fn with_state<R>(f: impl FnOnce(&mut UiState) -> R) -> R {
    let mut state = UI_STATE.lock().unwrap();
    f(&mut state)
}

pub(super) fn sync_to_webview() {
    if let Some(payload) = payload_json() {
        let escaped = payload.replace('\\', "\\\\").replace('`', "\\`");
        let script = format!(
            "if (window.__TTS_SET_STATE__) window.__TTS_SET_STATE__(JSON.parse(`{escaped}`));"
        );
        super::WEBVIEW.with(|slot| {
            if let Some(webview) = slot.borrow().as_ref() {
                let _ = webview.evaluate_script(&script);
            }
        });
    }
}

pub(super) fn payload_json() -> Option<String> {
    let app = crate::APP.lock().ok()?;
    let config = app.config.clone();
    drop(app);
    let ui = UI_STATE.lock().ok()?;
    let dark_mode = crate::overlay::is_dark_mode();
    let lang = config.ui_language.clone();
    let text = LocaleText::get(&lang);
    let payload = WebPayload::from_config(&config, &ui, dark_mode, &lang, &text);
    serde_json::to_string(&payload).ok()
}

// ---------------------------------------------------------------------------
// Serialization payload
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebPayload {
    theme: &'static str,
    ui_language: String,
    mode: String,
    method: String,
    draft_text: String,
    gemini: serde_json::Value,
    edge: serde_json::Value,
    google: serde_json::Value,
    step_audio: serde_json::Value,
    magpie: serde_json::Value,
    kokoro: serde_json::Value,
    supertonic: serde_json::Value,
    vieneu: serde_json::Value,
    audio_edit: serde_json::Value,
    s2s_target_language: String,
    player: PlayerView,
    catalogs: CatalogsView,
    strings: StringsView,
}

impl WebPayload {
    fn from_config(
        config: &Config,
        ui: &UiState,
        dark_mode: bool,
        lang: &str,
        text: &LocaleText,
    ) -> Self {
        let pg = &config.tts_playground;
        let edge = &pg.edge_settings;
        let magpie = &pg.magpie_settings;
        let kokoro = &pg.kokoro_settings;
        let supertonic = &pg.supertonic_settings;
        let step_audio = &pg.step_audio_settings;
        let vieneu = &pg.vieneu_settings;
        let audio_edit = &pg.step_audio_edit_settings;

        Self {
            theme: if dark_mode { "dark" } else { "light" },
            ui_language: lang.to_string(),
            mode: format!("{:?}", pg.mode),
            method: format!("{:?}", pg.method),
            draft_text: pg.draft_text.clone(),
            gemini: serde_json::json!({
                "model": pg.gemini_model,
                "voice": pg.gemini_voice,
                "speed": format!("{:?}", pg.gemini_speed),
                "instruction": pg.gemini_instruction,
                "conditions": pg
                    .gemini_language_conditions
                    .iter()
                    .map(|condition| serde_json::json!({
                        "language": condition.language_code,
                        "name": condition.language_name,
                        "instruction": condition.instruction,
                    }))
                    .collect::<Vec<_>>(),
            }),
            edge: serde_json::json!({
                "pitch": edge.pitch,
                "rate": edge.rate,
                "voices": edge
                    .voice_configs
                    .iter()
                    .map(|c| serde_json::json!({
                        "language": c.language_code,
                        "voice": c.voice_name,
                    }))
                    .collect::<Vec<_>>(),
            }),
            google: serde_json::json!({
                "speed": format!("{:?}", pg.google_speed),
            }),
            step_audio: serde_json::json!({
                "reference": step_audio.reference_voice_id,
            }),
            magpie: serde_json::json!({
                "speed": 1.0,
                "threads": 1,
                "voices": magpie
                    .voice_configs
                    .iter()
                    .map(|c| serde_json::json!({
                        "language": c.language_code,
                        "voice": c.voice_id,
                    }))
                    .collect::<Vec<_>>(),
            }),
            kokoro: serde_json::json!({
                "speed": kokoro.speed,
                "threads": kokoro.num_threads,
                "voices": kokoro
                    .voice_configs
                    .iter()
                    .map(|c| serde_json::json!({
                        "language": c.language_code,
                        "voice": c.voice_id,
                    }))
                    .collect::<Vec<_>>(),
            }),
            supertonic: serde_json::json!({
                "speed": supertonic.speed,
                "threads": supertonic.num_threads,
                "steps": supertonic.num_steps,
                "voices": supertonic
                    .voice_configs
                    .iter()
                    .map(|c| serde_json::json!({
                        "language": c.language_code,
                        "voice": c.voice_id,
                    }))
                    .collect::<Vec<_>>(),
            }),
            vieneu: serde_json::json!({
                "reference": vieneu.reference_voice_id,
            }),
            audio_edit: serde_json::json!({
                "sourcePath": audio_edit.source_audio_path,
                "sourceText": audio_edit.source_text,
                "targetText": audio_edit.target_text,
                "editType": audio_edit.edit_type,
                "editInfo": audio_edit.edit_info,
            }),
            s2s_target_language: normalize_s2s_language(&config.realtime_target_language),
            player: PlayerView {
                is_generating: ui.is_generating,
                is_exporting: ui.is_exporting,
                is_mic_recording: ui.is_mic_recording,
                is_playing: ui.is_playing,
                paused: ui.paused,
                position_sec: ui.position_sec,
                status: ui.status.clone(),
                error: ui.error.clone(),
                current: ui.current.clone(),
                recent: ui.recent.clone(),
            },
            catalogs: CatalogsView::from_config(config, text),
            strings: StringsView::from_locale(text),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerView {
    is_generating: bool,
    is_exporting: bool,
    is_mic_recording: bool,
    is_playing: bool,
    paused: bool,
    position_sec: f32,
    status: String,
    error: Option<String>,
    current: Option<CurrentClip>,
    recent: Vec<RecentClip>,
}

fn normalize_s2s_language(value: &str) -> String {
    match value {
        "English" => "en",
        "Vietnamese" => "vi",
        "Korean" => "ko",
        "Japanese" => "ja",
        "Chinese" => "zh",
        "Spanish" => "es",
        "French" => "fr",
        "German" => "de",
        other => other,
    }
    .to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StringsView {
    title: String,
    mode_tts_clone: String,
    mode_audio_edit: String,
    mode_reference_library: String,
    mode_s2_s: String,
    method_label: String,
    method_gemini: String,
    method_edge: String,
    method_google: String,
    method_step_audio: String,
    method_magpie: String,
    method_kokoro: String,
    method_supertonic: String,
    method_vieneu: String,
    text_label: String,
    text_hint: String,
    generate: String,
    clear: String,
    cancel: String,
    generating: String,
    exporting: String,
    no_audio: String,
    play: String,
    pause: String,
    resume: String,
    stop: String,
    replay: String,
    download_wav: String,
    download_mp3: String,
    recent: String,
    voice_per_language: String,
    add_language: String,
    reset: String,
    speed_label: String,
    speed_slow: String,
    speed_normal: String,
    speed_fast: String,
    pitch_label: String,
    rate_label: String,
    threads_label: String,
    quality_steps_label: String,
    pick_source: String,
    use_current: String,
    record_mic: String,
    stop_mic: String,
    no_source: String,
    source_transcript: String,
    task: String,
    subtask: String,
    inline_sound_tag: String,
    insert_tag: String,
    target_text: String,
    reference_voice: String,
    reference_library_desc: String,
    reference_add: String,
    reference_label: String,
    reference_pick_audio: String,
    reference_auto_recognize: String,
    reference_use_playground: String,
    reference_use_global: String,
    reference_no_audio: String,
    reference_exact_transcript: String,
    gemini_model_label: String,
    instructions_label: String,
    char_count_template: String,
}

impl StringsView {
    fn from_locale(text: &LocaleText) -> Self {
        Self {
            title: text.tts_playground_title.to_string(),
            mode_tts_clone: text.tts_playground_tab_tts_clone.to_string(),
            mode_audio_edit: text.tts_playground_tab_audio_edit.to_string(),
            mode_reference_library: text.tts_reference_voice_library_title.to_string(),
            mode_s2_s: "S2S".to_string(),
            method_label: text.tts_method_label.to_string(),
            method_gemini: text.tts_method_standard.to_string(),
            method_edge: text.tts_method_edge.to_string(),
            method_google: text.tts_method_fast.to_string(),
            method_step_audio: "Step Audio EditX".to_string(),
            method_magpie: "NVIDIA Magpie-Multilingual 357M".to_string(),
            method_kokoro: "Kokoro 82M v1.0".to_string(),
            method_supertonic: "Supertonic 3".to_string(),
            method_vieneu: "VieNeu-TTS v2".to_string(),
            text_label: text.tts_playground_text_label.to_string(),
            text_hint: text.tts_playground_text_hint.to_string(),
            generate: text.tts_playground_generate.to_string(),
            clear: text.tts_playground_clear.to_string(),
            cancel: text.cancel_label.to_string(),
            generating: text.tts_playground_generating.to_string(),
            exporting: text.tts_playground_exporting_mp3.to_string(),
            no_audio: text.tts_playground_no_audio.to_string(),
            play: text.tts_playground_play.to_string(),
            pause: text.tts_playground_pause.to_string(),
            resume: text.tts_playground_resume.to_string(),
            stop: text.tts_playground_stop.to_string(),
            replay: text.tts_playground_replay.to_string(),
            download_wav: text.tts_playground_download_wav.to_string(),
            download_mp3: text.tts_playground_download_mp3.to_string(),
            recent: text.tts_playground_recent.to_string(),
            voice_per_language: text.tts_voice_per_language_label.to_string(),
            add_language: "Add language".to_string(),
            reset: "Reset".to_string(),
            speed_label: text.tts_speed_label.to_string(),
            speed_slow: text.tts_speed_slow.to_string(),
            speed_normal: text.tts_speed_normal.to_string(),
            speed_fast: text.tts_speed_fast.to_string(),
            pitch_label: text.tts_pitch_label.to_string(),
            rate_label: text.tts_rate_label.to_string(),
            threads_label: "Threads".to_string(),
            quality_steps_label: "Quality steps".to_string(),
            pick_source: text.tts_step_audio_pick_source.to_string(),
            use_current: text.tts_step_audio_use_current_clip.to_string(),
            record_mic: text.tts_reference_record_mic.to_string(),
            stop_mic: text.tts_reference_stop_mic.to_string(),
            no_source: text.tts_step_audio_no_source.to_string(),
            source_transcript: text.tts_step_audio_source_transcript.to_string(),
            task: text.tts_step_audio_task.to_string(),
            subtask: text.tts_step_audio_subtask.to_string(),
            inline_sound_tag: text.tts_step_audio_inline_sound_tag.to_string(),
            insert_tag: text.tts_step_audio_insert_tag.to_string(),
            target_text: text.tts_step_audio_target_text.to_string(),
            reference_voice: text
                .tts_reference_voice_label
                .trim_end_matches(':')
                .to_string(),
            reference_library_desc: text.tts_reference_voice_library_desc.to_string(),
            reference_add: text.tts_reference_add.to_string(),
            reference_label: text.tts_reference_label.to_string(),
            reference_pick_audio: text.tts_reference_pick_audio.to_string(),
            reference_auto_recognize: text.tts_reference_auto_recognize.to_string(),
            reference_use_playground: text.tts_reference_use_playground.to_string(),
            reference_use_global: text.tts_reference_use_global.to_string(),
            reference_no_audio: text.tts_reference_no_audio.to_string(),
            reference_exact_transcript: text.tts_reference_exact_transcript.to_string(),
            gemini_model_label: text.tts_gemini_model_label.to_string(),
            instructions_label: text.tts_instructions_label.to_string(),
            char_count_template: "{n}".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Enum mapping helpers used by the IPC dispatcher
// ---------------------------------------------------------------------------

pub(super) fn parse_mode(value: &str) -> Option<crate::config::TtsPlaygroundMode> {
    use crate::config::TtsPlaygroundMode as M;
    Some(match value {
        "TtsClone" => M::TtsClone,
        "AudioEdit" => M::AudioEdit,
        "ReferenceLibrary" => M::ReferenceLibrary,
        "SpeechToSpeech" => M::SpeechToSpeech,
        _ => return None,
    })
}

pub(super) fn parse_method(value: &str) -> Option<crate::config::TtsMethod> {
    use crate::config::TtsMethod as M;
    Some(match value {
        "GeminiLive" => M::GeminiLive,
        "EdgeTTS" => M::EdgeTTS,
        "GoogleTranslate" => M::GoogleTranslate,
        "StepAudioEditX" => M::StepAudioEditX,
        "MagpieMultilingual" => M::MagpieMultilingual,
        "Kokoro" => M::Kokoro,
        "Supertonic" => M::Supertonic,
        "VieneuTts" => M::VieneuTts,
        _ => return None,
    })
}
