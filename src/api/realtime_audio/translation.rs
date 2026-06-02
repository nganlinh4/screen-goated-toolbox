//! Translation loop for realtime audio

mod providers;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::APP;
use crate::config::Preset;

use super::state::{SharedRealtimeState, TranslationRequest};
use super::utils::{refresh_transcription_window, update_translation_text};
use super::{TRANSLATION_INTERVAL_MS, WM_MODEL_SWITCH};
pub use providers::translate_with_google_gtx;
use providers::{TranslationKeys, ValidatedTranslationResponse, translate_with_provider};

const TRANSLATION_INTERVAL_MAX_MS: u64 = 4_000;

fn is_gemini_s2s_selected() -> bool {
    APP.lock()
        .map(|app| {
            crate::model_config::normalize_realtime_transcription_model_id(
                &app.config.realtime_transcription_model,
            ) == "gemini-live-s2s"
        })
        .unwrap_or(false)
}

/// Translation loop using the centralized realtime translation provider model.
pub fn run_translation_loop(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    translation_hwnd_send: crate::win_types::SendHwnd,
    state: SharedRealtimeState,
) {
    let translation_hwnd = translation_hwnd_send.0;
    let mut interval_ms = TRANSLATION_INTERVAL_MS;
    let mut last_run = Instant::now();

    if is_gemini_s2s_selected() {
        crate::log_info!(
            "[RealtimeTranslate] not starting text translation loop: Gemini S2S is active"
        );
        return;
    }

    let translation_block = match preset.blocks.get(1) {
        Some(b) => b.clone(),
        None => return,
    };

    let mut target_language = {
        let from_ui = crate::overlay::realtime_webview::NEW_TARGET_LANGUAGE
            .lock()
            .ok()
            .and_then(|lang| {
                if lang.is_empty() {
                    None
                } else {
                    Some(lang.clone())
                }
            });

        from_ui.unwrap_or_else(|| {
            if !translation_block.selected_language.is_empty() {
                translation_block.selected_language.clone()
            } else {
                translation_block
                    .language_vars
                    .get("language")
                    .cloned()
                    .or_else(|| translation_block.language_vars.get("language1").cloned())
                    .unwrap_or_else(|| "English".to_string())
            }
        })
    };

    while !stop_signal.load(Ordering::Relaxed) {
        if is_gemini_s2s_selected() {
            crate::log_info!(
                "[RealtimeTranslate] stopping text translation loop: switched to Gemini S2S"
            );
            break;
        }

        if translation_hwnd.0 != 0 as _ && !unsafe { IsWindow(Some(translation_hwnd)).as_bool() } {
            break;
        }

        if crate::overlay::realtime_webview::LANGUAGE_CHANGE.load(Ordering::SeqCst) {
            if let Ok(new_lang) = crate::overlay::realtime_webview::NEW_TARGET_LANGUAGE.lock()
                && !new_lang.is_empty()
            {
                target_language = new_lang.clone();
                if let Ok(mut s) = state.lock() {
                    s.translation_history.clear();
                }
            }
            crate::overlay::realtime_webview::LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
        }

        if crate::overlay::realtime_webview::TRANSLATION_MODEL_CHANGE.load(Ordering::SeqCst) {
            crate::overlay::realtime_webview::TRANSLATION_MODEL_CHANGE
                .store(false, Ordering::SeqCst);
        }

        {
            let should_force = { state.lock().unwrap().should_force_commit_on_timeout() };
            if should_force && let Ok(mut s) = state.lock() {
                s.force_commit_all();
                let display = s.display_translation.clone();
                update_translation_text(translation_hwnd, &display);
                refresh_transcription_window();
                last_run = Instant::now()
                    .checked_sub(Duration::from_millis(interval_ms))
                    .unwrap_or_else(Instant::now);
            }
        }

        if last_run.elapsed() >= Duration::from_millis(interval_ms) {
            if !crate::overlay::realtime_webview::TRANS_VISIBLE.load(Ordering::SeqCst) {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            let request = {
                let s = state.lock().unwrap();
                if s.is_transcript_unchanged() {
                    None
                } else {
                    s.get_translation_request()
                }
            };

            let Some(request) = request else {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            };

            let started_at = Instant::now();
            let (keys, translation_model, text_chain, config_snapshot, history_entries) = {
                let app = APP.lock().unwrap();
                let keys = TranslationKeys {
                    gemini: app.config.gemini_api_key.clone(),
                    cerebras: app.config.cerebras_api_key.clone(),
                    groq: app.config.api_key.clone(),
                };
                let model = app.config.realtime_translation_model.clone();
                let chain = app.config.model_priority_chains.text_to_text.clone();
                let cfg = app.config.clone();
                drop(app);
                let history = if let Ok(s) = state.lock() {
                    s.translation_history.clone()
                } else {
                    Vec::new()
                };
                (keys, model, chain, cfg, history)
            };

            let current_model = translation_model.as_str();
            let translation = translate_with_provider(
                current_model,
                &keys,
                &request,
                &target_language,
                &history_entries,
                &text_chain,
                &config_snapshot,
            );

            let applied = if let Some(result) = translation {
                apply_translation_update(&state, translation_hwnd, &request, &result)
            } else {
                false
            };

            if applied {
                let latency_ms = started_at.elapsed().as_millis() as u64;
                interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
            } else {
                let fallback_applied = handle_fallback_translation(FallbackTranslationRequest {
                    request: &request,
                    target_language: &target_language,
                    current_model,
                    keys: &keys,
                    history_entries: &history_entries,
                    text_chain: &text_chain,
                    config: &config_snapshot,
                    translation_hwnd,
                    state: &state,
                });

                if fallback_applied {
                    let latency_ms = started_at.elapsed().as_millis() as u64;
                    interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
                } else {
                    interval_ms = (interval_ms + 250).min(TRANSLATION_INTERVAL_MAX_MS);
                }
            }

            last_run = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

struct FallbackTranslationRequest<'a> {
    request: &'a TranslationRequest,
    target_language: &'a str,
    current_model: &'a str,
    keys: &'a TranslationKeys,
    history_entries: &'a [(String, String)],
    text_chain: &'a [String],
    config: &'a crate::config::Config,
    translation_hwnd: HWND,
    state: &'a SharedRealtimeState,
}

fn apply_translation_update(
    state: &SharedRealtimeState,
    translation_hwnd: HWND,
    request: &TranslationRequest,
    response: &ValidatedTranslationResponse,
) -> bool {
    if let Ok(mut s) = state.lock()
        && s.apply_translation_result(
            request,
            &response.finalized_translation,
            &response.draft_translation,
        )
    {
        let display = s.display_translation.clone();
        update_translation_text(translation_hwnd, &display);
        refresh_transcription_window();
        return true;
    }

    false
}

fn handle_fallback_translation(request: FallbackTranslationRequest<'_>) -> bool {
    let FallbackTranslationRequest {
        request,
        target_language,
        current_model,
        keys,
        history_entries,
        text_chain,
        config,
        translation_hwnd,
        state,
    } = request;

    let alt_model = if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        crate::model_config::REALTIME_TRANSLATION_MODEL_LLM
    } else {
        crate::model_config::REALTIME_TRANSLATION_MODEL_GTX
    };

    {
        let mut app = APP.lock().unwrap();
        app.config.realtime_translation_model = alt_model.to_string();
        crate::config::save_config(&app.config);
    }
    unsafe {
        let flag = match alt_model {
            crate::model_config::REALTIME_TRANSLATION_MODEL_GTX => 1,
            _ => 0,
        };
        let _ = PostMessageW(
            Some(translation_hwnd),
            WM_MODEL_SWITCH,
            WPARAM(flag),
            LPARAM(0),
        );
    }

    let translated = translate_with_provider(
        alt_model,
        keys,
        request,
        target_language,
        history_entries,
        text_chain,
        config,
    );

    if let Some(result) = translated
        && apply_translation_update(state, translation_hwnd, request, &result)
    {
        return true;
    }

    false
}

fn compute_adaptive_translation_interval_ms(latency_ms: u64) -> u64 {
    (latency_ms + 250).clamp(TRANSLATION_INTERVAL_MS, TRANSLATION_INTERVAL_MAX_MS)
}
