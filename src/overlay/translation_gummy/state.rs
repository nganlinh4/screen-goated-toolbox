use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::TranslationGummySettings;
use crate::gui::locale::LocaleText;

use super::{TranslationGummyConnectionState, TranslationGummyTranscriptItem};

lazy_static::lazy_static! {
    static ref UI_STATE: Mutex<UiState> = Mutex::new(UiState::from_config());
}

#[derive(Clone)]
pub(super) struct UiState {
    pub(super) applied: TranslationGummySettings,
    pub(super) draft: TranslationGummySettings,
    pub(super) dirty: bool,
    pub(super) is_running: bool,
    pub(super) connection_state: TranslationGummyConnectionState,
    pub(super) transcripts: Vec<TranslationGummyTranscriptItem>,
    pub(super) last_error: Option<String>,
    pub(super) hotkey_error: Option<String>,
    pub(super) audio_level: f32,
}

impl UiState {
    fn from_config() -> Self {
        let applied = super::current_settings();
        Self {
            draft: applied.clone(),
            applied,
            dirty: false,
            is_running: false,
            connection_state: TranslationGummyConnectionState::NotConfigured,
            transcripts: load_persisted_transcripts(),
            last_error: None,
            hotkey_error: None,
            audio_level: 0.0,
        }
    }

    pub(super) fn normalize(&mut self) {
        self.applied = self.applied.normalized();
        self.draft = self.draft.normalized();
        self.dirty = self.draft != self.applied;
        self.audio_level = self.audio_level.clamp(0.0, 1.0);
        if !self.applied.is_valid() && !self.is_running {
            self.connection_state = TranslationGummyConnectionState::NotConfigured;
        }
        if !self.is_running {
            self.audio_level = 0.0;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebPayload {
    dark_mode: bool,
    status_label: String,
    connection_state: &'static str,
    is_running: bool,
    dirty: bool,
    can_apply: bool,
    can_toggle: bool,
    audio_level: f32,
    draft: TranslationGummySettings,
    hotkeys: Vec<crate::config::Hotkey>,
    hotkey_error: Option<String>,
    last_error: Option<String>,
    transcripts: Vec<TranslationGummyTranscriptItem>,
    guide_seen: bool,
    tts_model: String,
    tts_voice: String,
    strings: WebStrings,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebStrings {
    title: String,
    first_profile: String,
    second_profile: String,
    language_label: String,
    accent_label: String,
    tone_label: String,
    hotkey_label: String,
    set_hotkey: String,
    clear_hotkey: String,
    apply: String,
    start: String,
    stop: String,
    transcript_title: String,
    input_chip: String,
    output_chip: String,
    no_transcript: String,
    guide: String,
    guide_ok: String,
    chat_history: String,
    current_model: String,
    current_voice: String,
}

pub(super) fn with_state<R>(f: impl FnOnce(&mut UiState) -> R) -> R {
    let mut state = UI_STATE.lock().unwrap();
    f(&mut state)
}

pub(super) fn snapshot() -> UiState {
    UI_STATE.lock().unwrap().clone()
}

pub(super) fn refresh_from_config() {
    with_state(|state| {
        state.applied = super::current_settings();
        if !state.dirty {
            state.draft = state.applied.clone();
        }
        state.normalize();
    });
}

pub(super) fn insert_session_separator() {
    with_state(|state| {
        // Don't insert separator if transcripts are empty or last item is already a separator
        if state.transcripts.is_empty()
            || state
                .transcripts
                .last()
                .map(|t| t.role == "separator")
                .unwrap_or(false)
        {
            return;
        }
        let now = chrono::Local::now().format("%H:%M").to_string();
        state
            .transcripts
            .push(super::TranslationGummyTranscriptItem {
                id: super::runtime::next_transcript_id(),
                role: "separator",
                text: now,
                is_final: true,
                lang: String::new(),
            });
        // Keep max 200 items (100 pairs + separators)
        if state.transcripts.len() > 200 {
            let overflow = state.transcripts.len() - 200;
            state.transcripts.drain(0..overflow);
        }
        state.last_error = None;
    });
    persist_transcripts();
}

pub(super) fn publish_connection(
    connection_state: TranslationGummyConnectionState,
    is_running: bool,
    last_error: Option<String>,
) {
    with_state(|state| {
        state.connection_state = connection_state;
        state.is_running = is_running;
        state.last_error = last_error;
        state.hotkey_error = None;
        if !is_running {
            state.audio_level = 0.0;
        }
        state.normalize();
    });
}

pub(super) fn publish_error(
    connection_state: TranslationGummyConnectionState,
    error: String,
    is_running: bool,
) {
    with_state(|state| {
        state.connection_state = connection_state;
        state.last_error = Some(error);
        state.is_running = is_running;
        state.audio_level = 0.0;
        state.normalize();
    });
}

pub(super) fn publish_audio_level(level: f32) {
    with_state(|state| {
        state.audio_level = level.clamp(0.0, 1.0);
        state.normalize();
    });
}

fn detect_lang(text: &str) -> String {
    crate::lang_detect::detect_language(text).unwrap_or_default()
}

pub(super) fn upsert_transcript(role: &'static str, text: String, is_final: bool) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    with_state(|state| {
        let dominated_by_same = state
            .transcripts
            .last()
            .map(|last| last.role == role)
            .unwrap_or(false);
        // Try to find an unfinal item of the same role to merge into
        if let Some(existing) = state
            .transcripts
            .iter_mut()
            .rev()
            .find(|item| item.role == role && !item.is_final)
        {
            existing.text = merge_transcript_text(&existing.text, text);
            existing.is_final = is_final;
            // Only detect language on complete text — partial streaming fragments
            // are too short for reliable language detection
            if is_final {
                let detected = detect_lang(&existing.text);
                if !detected.is_empty() {
                    existing.lang = detected;
                }
            }
        }
        // If no unfinal item, check if the last item of same role was JUST finalized
        // (Gemini sometimes splits long translations into multiple chunks after turnComplete)
        else if let Some(last_same) = state
            .transcripts
            .iter_mut()
            .rev()
            .find(|item| item.role == role && item.is_final)
            .filter(|_| dominated_by_same)
        {
            last_same.text = merge_transcript_text(&last_same.text, text);
            let detected = detect_lang(&last_same.text);
            if !detected.is_empty() {
                last_same.lang = detected;
            }
        } else {
            // New item — only detect if already final (single-shot transcript)
            let lang = if is_final {
                detect_lang(text)
            } else {
                String::new()
            };
            state.transcripts.push(TranslationGummyTranscriptItem {
                id: super::runtime::next_transcript_id(),
                role,
                text: text.to_string(),
                is_final,
                lang,
            });
            if state.transcripts.len() > 200 {
                let overflow = state.transcripts.len() - 200;
                state.transcripts.drain(0..overflow);
            }
        }
    });
}

pub(super) fn finalize_transcripts() {
    with_state(|state| {
        for item in &mut state.transcripts {
            if !item.is_final {
                item.is_final = true;
                // Detect language on the now-complete text
                if item.lang.is_empty() {
                    let detected = detect_lang(&item.text);
                    if !detected.is_empty() {
                        item.lang = detected;
                    }
                }
            }
        }
    });
    persist_transcripts();
}

pub(super) fn request_sync() {
    unsafe {
        let hwnd = std::ptr::addr_of!(super::WINDOW_HWND).read();
        if !hwnd.is_invalid() {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(hwnd.0),
                super::WM_APP_SYNC,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

pub(super) fn payload_json() -> Option<String> {
    let ui_language = super::current_ui_language();
    let text = LocaleText::get(&ui_language);
    let dark_mode = crate::overlay::is_dark_mode();
    let state = snapshot();
    let payload = WebPayload {
        dark_mode,
        status_label: super::status_label(&text, state.connection_state).to_string(),
        connection_state: super::connection_key(state.connection_state),
        is_running: state.is_running,
        dirty: state.dirty,
        can_apply: state.draft.is_valid(),
        can_toggle: state.applied.is_valid(),
        audio_level: state.audio_level,
        draft: state.draft.clone(),
        hotkeys: state.draft.hotkeys.clone(),
        hotkey_error: state.hotkey_error.clone(),
        last_error: state.last_error.clone().map(|err| match err.as_str() {
            "missing_api_key" => text.translation_gummy_api_key_required.to_string(),
            _ => err,
        }),
        transcripts: state.transcripts.clone(),
        guide_seen: crate::APP
            .lock()
            .map(|a| a.config.translation_gummy.guide_seen)
            .unwrap_or(true),
        tts_model: {
            let (m, _) = super::runtime::current_gemini_tts_settings();
            m
        },
        tts_voice: {
            let (_, v) = super::runtime::current_gemini_tts_settings();
            v
        },
        strings: WebStrings {
            title: text.translation_gummy_title.to_string(),
            first_profile: text.translation_gummy_first_profile.to_string(),
            second_profile: text.translation_gummy_second_profile.to_string(),
            language_label: text.translation_gummy_language_label.to_string(),
            accent_label: text.translation_gummy_accent_label.to_string(),
            tone_label: text.translation_gummy_tone_label.to_string(),
            hotkey_label: text.translation_gummy_hotkey_label.to_string(),
            set_hotkey: text.translation_gummy_hotkey_set.to_string(),
            clear_hotkey: text.translation_gummy_hotkey_clear.to_string(),
            apply: text.translation_gummy_apply.to_string(),
            start: text.translation_gummy_start.to_string(),
            stop: text.translation_gummy_stop.to_string(),
            transcript_title: text.translation_gummy_transcript_title.to_string(),
            input_chip: text.translation_gummy_input_chip.to_string(),
            output_chip: text.translation_gummy_output_chip.to_string(),
            no_transcript: text.translation_gummy_no_transcript_yet.to_string(),
            guide: text.translation_gummy_guide.to_string(),
            guide_ok: text.translation_gummy_guide_ok.to_string(),
            chat_history: text.translation_gummy_chat_history.to_string(),
            current_model: text.translation_gummy_current_model.to_string(),
            current_voice: text.translation_gummy_current_voice.to_string(),
        },
    };
    serde_json::to_string(&payload).ok()
}

pub(super) fn sync_to_webview() {
    let Some(payload_json) = payload_json() else {
        return;
    };
    let script = format!(
        "window.__TG_SET_STATE && window.__TG_SET_STATE({payload_json});"
    );
    super::WEBVIEW.with(|webview| {
        if let Some(webview) = webview.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}

fn merge_transcript_text(existing: &str, incoming: &str) -> String {
    let current = existing.trim();
    let next = incoming.trim();
    if current.is_empty() {
        return next.to_string();
    }
    if next.is_empty() {
        return current.to_string();
    }
    if next.starts_with(current) || next.contains(current) {
        return next.to_string();
    }
    if current.starts_with(next) || current.contains(next) || current.ends_with(next) {
        return current.to_string();
    }
    if current.ends_with(' ')
        || next.starts_with(' ')
        || matches!(
            next.chars().next(),
            Some(',' | '.' | '!' | '?' | ':' | ';' | ')' | ']' | '}')
        )
    {
        return format!("{current}{next}");
    }
    format!("{current} {next}")
}

// ── Transcript persistence ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct PersistedTranscript {
    role: String,
    text: String,
    lang: String,
}

fn transcripts_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("translation_gummy_transcripts.json")
}

fn legacy_transcripts_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("bilingual_relay_transcripts.json")
}

pub(super) fn persist_transcripts() {
    let items: Vec<PersistedTranscript> = {
        let state = UI_STATE.lock().unwrap();
        state
            .transcripts
            .iter()
            .filter(|t| t.is_final)
            .map(|t| PersistedTranscript {
                role: t.role.to_string(),
                text: t.text.clone(),
                lang: t.lang.clone(),
            })
            .collect()
    };
    if let Ok(json) = serde_json::to_string(&items) {
        let _ = std::fs::write(transcripts_path(), json);
    }
}

fn load_persisted_transcripts() -> Vec<TranslationGummyTranscriptItem> {
    let path = if transcripts_path().exists() {
        transcripts_path()
    } else {
        legacy_transcripts_path()
    };
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let items: Vec<PersistedTranscript> = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    items
        .into_iter()
        .map(|p| {
            let role: &'static str = match p.role.as_str() {
                "input" => "input",
                "output" => "output",
                "separator" => "separator",
                _ => "input",
            };
            TranslationGummyTranscriptItem {
                id: super::runtime::next_transcript_id(),
                role,
                text: p.text,
                is_final: true,
                lang: p.lang,
            }
        })
        .collect()
}
