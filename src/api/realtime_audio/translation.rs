//! Translation loop for realtime audio

mod provider_cooldown;
mod providers;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::APP;

use super::TRANSLATION_INTERVAL_MS;
use super::state::{SharedRealtimeState, TranslationRequest};
use super::utils::{notify_translation_changed, refresh_transcription_window};
use provider_cooldown::{ProviderCooldown, alternate_provider};
pub use providers::translate_with_google_gtx;
use providers::{TranslationKeys, ValidatedTranslationResponse, translate_with_provider};

const TRANSLATION_INTERVAL_MAX_MS: u64 = 4_000;

fn is_gemini_s2s_selected() -> bool {
    APP.lock()
        .map(|app| {
            crate::model_config::is_gemini_live_s2s_model_id(
                &app.config.realtime_transcription_model,
            )
        })
        .unwrap_or(false)
}

/// Translation loop using the centralized realtime translation provider model.
pub fn run_translation_loop(
    initial_target_language: String,
    stop_signal: Arc<AtomicBool>,
    translation_hwnd_send: crate::win_types::SendHwnd,
    state: SharedRealtimeState,
    session_id: u64,
) {
    let translation_hwnd = translation_hwnd_send.0;
    let mut interval_ms = TRANSLATION_INTERVAL_MS;
    let mut last_run = Instant::now();
    let mut cooldown = ProviderCooldown::default();

    if is_gemini_s2s_selected() {
        crate::log_info!(
            "[RealtimeTranslate] not starting text translation loop: direct speech model is active"
        );
        return;
    }

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
            if initial_target_language.is_empty() {
                "English".to_string()
            } else {
                initial_target_language
            }
        })
    };

    while !stop_signal.load(Ordering::Relaxed) {
        if is_gemini_s2s_selected() {
            crate::log_info!(
                "[RealtimeTranslate] stopping text translation loop: switched to direct speech model"
            );
            break;
        }

        if translation_hwnd.0 != 0 as _ && !unsafe { IsWindow(Some(translation_hwnd)).as_bool() } {
            break;
        }

        let mut method_ready = false;
        if !crate::overlay::realtime_webview::state::publish_if_current(
            session_id,
            &stop_signal,
            || {
                if !text_translation_allowed(
                    is_gemini_s2s_selected(),
                    crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst),
                    crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE
                        .load(Ordering::SeqCst),
                    crate::overlay::realtime_webview::LANGUAGE_CHANGE.load(Ordering::SeqCst),
                    super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst),
                ) {
                    return;
                }
                method_ready = true;
                if crate::overlay::realtime_webview::TRANSLATION_MODEL_CHANGE.load(Ordering::SeqCst)
                {
                    crate::overlay::realtime_webview::TRANSLATION_MODEL_CHANGE
                        .store(false, Ordering::SeqCst);
                }

                {
                    let should_force = { state.lock().unwrap().should_force_commit_on_timeout() };
                    if should_force && let Ok(mut s) = state.lock() {
                        s.force_commit_all();
                        notify_translation_changed(translation_hwnd);
                        refresh_transcription_window();
                        last_run = Instant::now()
                            .checked_sub(Duration::from_millis(interval_ms))
                            .unwrap_or_else(Instant::now);
                    }
                }
            },
        ) {
            break;
        }
        if !method_ready {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        if last_run.elapsed() >= Duration::from_millis(interval_ms) {
            if !crate::overlay::realtime_webview::TRANS_VISIBLE.load(Ordering::SeqCst) {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            let mut request = None;
            let mut translation_revision = 0;
            let mut options_revision = 0;
            if !crate::overlay::realtime_webview::state::publish_if_current(
                session_id,
                &stop_signal,
                || {
                    let s = state.lock().unwrap();
                    translation_revision = s.translation_revision;
                    options_revision =
                        crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
                            .load(Ordering::SeqCst);
                    if let Ok(language) =
                        crate::overlay::realtime_webview::NEW_TARGET_LANGUAGE.lock()
                        && !language.is_empty()
                    {
                        target_language = language.clone();
                    }
                    if !crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                        && !crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE
                            .load(Ordering::SeqCst)
                        && !s.is_transcript_unchanged()
                    {
                        request = s.get_translation_request();
                    }
                },
            ) {
                break;
            }

            let Some(request) = request else {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            };

            let started_at = Instant::now();
            let (keys, translation_model, text_chain, config_snapshot) = {
                let app = APP.lock().unwrap();
                let keys = TranslationKeys {
                    gemini: crate::api::provider_credentials::resolve(
                        "GEMINI_API_KEY",
                        &app.config.gemini_api_key,
                    ),
                    groq: crate::api::provider_credentials::resolve(
                        "GROQ_API_KEY",
                        &app.config.api_key,
                    ),
                    nvidia: crate::api::provider_credentials::resolve(
                        "NVIDIA_API_KEY",
                        &app.config.nvidia_api_key,
                    ),
                };
                let model = app.config.realtime_translation_model.clone();
                let cfg = app.config.clone();
                let chain =
                    crate::retry_model_chain::RetryChainKind::TextToText.effective_chain(&cfg);
                drop(app);
                (keys, model, chain, cfg)
            };
            let mut history_entries = Vec::new();
            let mut request_current = false;
            if !crate::overlay::realtime_webview::state::publish_if_current(
                session_id,
                &stop_signal,
                || {
                    if let Ok(s) = state.lock()
                        && translation_request_is_current(&s, &request, translation_revision)
                        && options_are_current(options_revision)
                    {
                        history_entries = s.translation_history.clone();
                        request_current = true;
                    }
                },
            ) {
                break;
            }
            if !request_current {
                continue;
            }

            let current_model = translation_model.as_str();
            let provider = cooldown.provider_for(current_model, Instant::now());
            let provider_is_configured = provider == current_model;
            let translation = translate_with_provider(
                provider,
                &keys,
                &request,
                &target_language,
                &history_entries,
                &text_chain,
                &config_snapshot,
            );

            if provider_is_configured {
                if translation.is_some() {
                    cooldown.record_success(current_model);
                } else {
                    cooldown.record_failure(current_model, Instant::now());
                }
            }

            if !crate::overlay::realtime_webview::state::publish_if_current(
                session_id,
                &stop_signal,
                || {},
            ) {
                break;
            }

            let provider_answered = translation.is_some();
            let applied = if let Some(result) = translation {
                apply_translation_update(
                    &state,
                    translation_hwnd,
                    &request,
                    &result,
                    session_id,
                    &stop_signal,
                    translation_revision,
                    options_revision,
                    &crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION,
                )
            } else {
                false
            };

            if applied {
                let latency_ms = started_at.elapsed().as_millis() as u64;
                interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
            } else if !provider_is_configured {
                // The configured provider is cooling down and the alternate
                // already served this tick; an answer it could not apply
                // belongs to a retired request.
                if provider_answered {
                    continue;
                }
                interval_ms = (interval_ms + 250).min(TRANSLATION_INTERVAL_MAX_MS);
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
                    session_id,
                    stop_signal: &stop_signal,
                    translation_revision,
                    options_revision,
                });

                match fallback_applied {
                    Some(true) => {
                        let latency_ms = started_at.elapsed().as_millis() as u64;
                        interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
                    }
                    Some(false) => {
                        interval_ms = (interval_ms + 250).min(TRANSLATION_INTERVAL_MAX_MS);
                    }
                    None => continue,
                }
            }

            last_run = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn text_translation_allowed(
    direct_speech: bool,
    source_changed: bool,
    method_changed: bool,
    language_changed: bool,
    reconnect_requested: bool,
) -> bool {
    !direct_speech
        && !source_changed
        && !method_changed
        && !language_changed
        && !reconnect_requested
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
    session_id: u64,
    stop_signal: &'a AtomicBool,
    translation_revision: u64,
    options_revision: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "translation admission requires session and request identity"
)]
fn apply_translation_update(
    state: &SharedRealtimeState,
    translation_hwnd: HWND,
    request: &TranslationRequest,
    response: &ValidatedTranslationResponse,
    session_id: u64,
    stop_signal: &AtomicBool,
    translation_revision: u64,
    options_revision: u64,
    options: &std::sync::atomic::AtomicU64,
) -> bool {
    let mut applied = false;
    crate::overlay::realtime_webview::state::publish_if_current(session_id, stop_signal, || {
        if let Ok(mut s) = state.lock()
            && translation_admission_matches(
                &s,
                request,
                translation_revision,
                options_revision,
                options.load(Ordering::SeqCst),
            )
            && s.apply_translation_result(
                request,
                &response.finalized_translation,
                &response.draft_translation,
            )
        {
            notify_translation_changed(translation_hwnd);
            refresh_transcription_window();
            applied = true;
        }
    });
    applied
}

// None denotes a retired request, which is not a provider failure.
fn handle_fallback_translation(request: FallbackTranslationRequest<'_>) -> Option<bool> {
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
        session_id,
        stop_signal,
        translation_revision,
        options_revision,
    } = request;

    let current_fallback = || {
        let mut alt_model = None;
        crate::overlay::realtime_webview::state::publish_if_current(
            session_id,
            stop_signal,
            || {
                if !options_are_current(options_revision) {
                    return;
                }
                let app = APP.lock().unwrap();
                if state.lock().is_ok_and(|s| {
                    translation_request_is_current(&s, request, translation_revision)
                }) {
                    alt_model = fallback_model_if_current(
                        current_model,
                        &app.config.realtime_translation_model,
                    );
                }
            },
        );
        alt_model
    };
    let alt_model = current_fallback()?;

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
        && apply_translation_update(
            state,
            translation_hwnd,
            request,
            &result,
            session_id,
            stop_signal,
            translation_revision,
            options_revision,
            &crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION,
        )
    {
        return Some(true);
    }

    current_fallback().map(|_| false)
}

fn options_are_current(expected: u64) -> bool {
    options_match(
        expected,
        crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
            .load(Ordering::SeqCst),
    )
}

fn options_match(expected: u64, current: u64) -> bool {
    expected == current
}

fn translation_admission_matches(
    state: &super::state::RealtimeState,
    request: &TranslationRequest,
    translation_revision: u64,
    expected_options: u64,
    current_options: u64,
) -> bool {
    translation_request_is_current(state, request, translation_revision)
        && options_match(expected_options, current_options)
}

fn fallback_model_if_current(requested: &str, current: &str) -> Option<&'static str> {
    (requested == current).then(|| alternate_provider(requested))
}

fn translation_request_is_current(
    state: &super::state::RealtimeState,
    request: &TranslationRequest,
    revision: u64,
) -> bool {
    state.translation_revision == revision
        && request.matches_source(&state.full_transcript, state.last_committed_pos)
        && state.request_finality_is_current(request)
}

fn compute_adaptive_translation_interval_ms(latency_ms: u64) -> u64 {
    (latency_ms + 250).clamp(TRANSLATION_INTERVAL_MS, TRANSLATION_INTERVAL_MAX_MS)
}

#[cfg(test)]
#[path = "translation_tests.rs"]
mod tests;
