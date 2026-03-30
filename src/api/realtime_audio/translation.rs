//! Translation loop for realtime audio

use isolang;
use serde::Deserialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use urlencoding;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::config::Preset;

use super::state::{SharedRealtimeState, TranslationRequest};
use super::utils::{refresh_transcription_window, update_translation_text};
use super::{TRANSLATION_INTERVAL_MS, WM_MODEL_SWITCH};

const TRANSLATION_INTERVAL_MAX_MS: u64 = 4_000;

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
            let (gemini_key, cerebras_key, translation_model, history_entries) = {
                let app = APP.lock().unwrap();
                let gemini = app.config.gemini_api_key.clone();
                let cerebras = app.config.cerebras_api_key.clone();
                let model = app.config.realtime_translation_model.clone();
                drop(app);
                let history = if let Ok(s) = state.lock() {
                    s.translation_history.clone()
                } else {
                    Vec::new()
                };
                (gemini, cerebras, model, history)
            };

            let current_model = translation_model.as_str();
            let translation = translate_with_provider(
                current_model,
                &gemini_key,
                &cerebras_key,
                &request,
                &target_language,
                &history_entries,
            );

            let applied = if let Some(result) = translation {
                apply_translation_update(&state, translation_hwnd, &request, &result)
            } else {
                false
            };

            if applied {
                let latency_ms = started_at.elapsed().as_millis() as u64;
                interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
                eprintln!(
                    "Live translate success: provider={} range={}-{} latency={}ms next_interval={}ms",
                    current_model,
                    request.source_start,
                    request.source_end,
                    latency_ms,
                    interval_ms
                );
            } else {
                let fallback_applied = handle_fallback_translation(FallbackTranslationRequest {
                    request: &request,
                    target_language: &target_language,
                    current_model,
                    gemini_key: &gemini_key,
                    cerebras_key: &cerebras_key,
                    history_entries: &history_entries,
                    translation_hwnd,
                    state: &state,
                });

                if fallback_applied {
                    let latency_ms = started_at.elapsed().as_millis() as u64;
                    interval_ms = compute_adaptive_translation_interval_ms(latency_ms);
                } else {
                    interval_ms = (interval_ms + 250).min(TRANSLATION_INTERVAL_MAX_MS);
                    eprintln!(
                        "Live translate failure: provider={} range={}-{} next_interval={}ms",
                        current_model, request.source_start, request.source_end, interval_ms
                    );
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
    gemini_key: &'a str,
    cerebras_key: &'a str,
    history_entries: &'a [(String, String)],
    translation_hwnd: HWND,
    state: &'a SharedRealtimeState,
}

#[derive(Debug, Clone)]
struct ValidatedTranslationResponse {
    finalized_translation: String,
    draft_translation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationModelResponse {
    patches: Vec<TranslationModelPatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationModelPatch {
    source_start: usize,
    source_end: usize,
    state: String,
    translation: String,
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
        gemini_key,
        cerebras_key,
        history_entries,
        translation_hwnd,
        state,
    } = request;

    let alt_model = if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_CEREBRAS {
        crate::model_config::REALTIME_TRANSLATION_MODEL_GTX
    } else if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        crate::model_config::REALTIME_TRANSLATION_MODEL_CEREBRAS
    } else {
        let pool = [
            crate::model_config::REALTIME_TRANSLATION_MODEL_CEREBRAS,
            crate::model_config::REALTIME_TRANSLATION_MODEL_GTX,
        ];
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        pool[(nanos as usize) % pool.len()]
    };

    {
        let mut app = APP.lock().unwrap();
        app.config.realtime_translation_model = alt_model.to_string();
        crate::config::save_config(&app.config);
    }
    unsafe {
        let flag = match alt_model {
            crate::model_config::REALTIME_TRANSLATION_MODEL_GEMMA => 1,
            crate::model_config::REALTIME_TRANSLATION_MODEL_GTX => 2,
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
        gemini_key,
        cerebras_key,
        request,
        target_language,
        history_entries,
    );

    if let Some(result) = translated
        && apply_translation_update(state, translation_hwnd, request, &result)
    {
        eprintln!(
            "Live translate fallback success: provider={} range={}-{}",
            alt_model, request.source_start, request.source_end
        );
        return true;
    }

    false
}

fn translate_with_provider(
    current_model: &str,
    gemini_key: &str,
    cerebras_key: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> Option<ValidatedTranslationResponse> {
    eprintln!(
        "Live translate request: provider={} range={}-{} finalize={} draft={}",
        current_model,
        request.source_start,
        request.source_end,
        request.bytes_to_commit(),
        request.draft_source.len()
    );

    if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        return translate_with_google_gtx_request(request, target_language);
    }

    if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GEMMA {
        let model_name = crate::model_config::realtime_translation_api_model(current_model);
        return translate_with_google_model(
            gemini_key,
            model_name,
            request,
            target_language,
            history_entries,
        );
    }

    let model_name = crate::model_config::realtime_translation_api_model(current_model);
    translate_with_cerebras(
        cerebras_key,
        model_name,
        request,
        target_language,
        history_entries,
        current_model,
    )
}

fn translate_with_google_model(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> Option<ValidatedTranslationResponse> {
    if api_key.trim().is_empty() {
        return None;
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model_name
    );
    let payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{
                "text": build_structured_prompt(request, target_language, history_entries)
            }]
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let resp = UREQ_AGENT
        .post(&url)
        .header("x-goog-api-key", api_key)
        .send_json(payload)
        .ok()?;

    let root: serde_json::Value = resp.into_body().read_json().ok()?;
    let parts = root
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?;

    let mut json_text = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
            json_text.push_str(text);
        }
    }

    parse_translation_response(&json_text, request)
}

fn translate_with_cerebras(
    api_key: &str,
    model_name: &str,
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
    current_model: &str,
) -> Option<ValidatedTranslationResponse> {
    if api_key.trim().is_empty() {
        return None;
    }

    let payload = serde_json::json!({
        "model": model_name,
        "messages": build_cerebras_messages(request, target_language, history_entries),
        "stream": false,
        "max_tokens": 512,
        "response_format": cerebras_response_format(),
    });

    let resp = UREQ_AGENT
        .post("https://api.cerebras.ai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .ok()?;

    if let Some(remaining) = resp
        .headers()
        .get("x-ratelimit-remaining-requests-tokens")
        .and_then(|v| v.to_str().ok())
    {
        let limit = resp
            .headers()
            .get("x-ratelimit-limit-tokens")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        if let Ok(mut app) = APP.lock() {
            app.model_usage_stats.insert(
                crate::model_config::realtime_translation_api_model(current_model).to_string(),
                format!("{} / {}", remaining, limit),
            );
        }
    }

    let root: serde_json::Value = resp.into_body().read_json().ok()?;
    let content = root
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()?;

    parse_translation_response(content, request)
}

fn translate_with_google_gtx_request(
    request: &TranslationRequest,
    target_language: &str,
) -> Option<ValidatedTranslationResponse> {
    let finalized_translation = if request.finalized_source.is_empty() {
        String::new()
    } else {
        translate_with_google_gtx(&request.finalized_source, target_language)?
    };

    let draft_translation = if request.draft_source.is_empty() {
        String::new()
    } else {
        translate_with_google_gtx(&request.draft_source, target_language)?
    };

    Some(ValidatedTranslationResponse {
        finalized_translation,
        draft_translation,
    })
}

fn parse_translation_response(
    payload: &str,
    request: &TranslationRequest,
) -> Option<ValidatedTranslationResponse> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let cleaned = without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim();

    let response: TranslationModelResponse = serde_json::from_str(cleaned).ok()?;
    validate_translation_response(response, request)
}

fn validate_translation_response(
    response: TranslationModelResponse,
    request: &TranslationRequest,
) -> Option<ValidatedTranslationResponse> {
    let finalized_translation = if request.finalized_source.is_empty() {
        String::new()
    } else {
        response
            .patches
            .iter()
            .find(|patch| {
                patch.state == "final"
                    && patch.source_start == request.source_start
                    && patch.source_end == request.finalized_source_end
                    && !patch.translation.trim().is_empty()
            })?
            .translation
            .trim()
            .to_string()
    };

    let draft_translation = if request.draft_source.is_empty() {
        String::new()
    } else {
        match response.patches.iter().find(|patch| {
            patch.state == "draft"
                && patch.source_start == request.draft_source_start()
                && patch.source_end == request.source_end
                && !patch.translation.trim().is_empty()
        }) {
            Some(patch) => patch.translation.trim().to_string(),
            None if !request.requires_draft_translation() => request.fallback_draft_translation(),
            None => return None,
        }
    };

    Some(ValidatedTranslationResponse {
        finalized_translation,
        draft_translation,
    })
}

fn build_cerebras_messages(
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    messages.push(serde_json::json!({
        "role": "system",
        "content": "You translate live transcript windows into JSON source patches. Respond with JSON only."
    }));
    for (source, translation) in history_entries {
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!("Translate to {}:\n{}", target_language, source)
        }));
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": translation
        }));
    }
    messages.push(serde_json::json!({
        "role": "user",
        "content": build_structured_prompt(request, target_language, history_entries)
    }));
    messages
}

fn build_structured_prompt(
    request: &TranslationRequest,
    target_language: &str,
    history_entries: &[(String, String)],
) -> String {
    let history_json: Vec<serde_json::Value> = history_entries
        .iter()
        .map(|(source, translation)| {
            serde_json::json!({
                "source": source,
                "translation": translation,
            })
        })
        .collect();

    let mut expected_patches = Vec::new();
    if !request.finalized_source.is_empty() {
        expected_patches.push(serde_json::json!({
            "sourceStart": request.source_start,
            "sourceEnd": request.finalized_source_end,
            "state": "final",
        }));
    }
    if !request.draft_source.is_empty() {
        expected_patches.push(serde_json::json!({
            "sourceStart": request.draft_source_start(),
            "sourceEnd": request.source_end,
            "state": "draft",
        }));
    }

    let window_json = serde_json::json!({
        "sourceStart": request.source_start,
        "sourceEnd": request.source_end,
        "pendingSource": request.pending_source,
        "finalizedSource": request.finalized_source,
        "draftSource": request.draft_source,
        "previousDraftTranslation": request.previous_draft_translation,
    });

    format!(
        "You are a professional live translator.\n\
Translate only the provided source window into {target_language}.\n\
Return JSON with a single key named patches.\n\
Each patch must keep the exact sourceStart/sourceEnd values from expectedPatches.\n\
Use state=\"final\" for the finalized source span and state=\"draft\" for the trailing unfinished span.\n\
Do not add commentary, markdown, or extra keys.\n\n\
Recent committed context:\n{history}\n\n\
Current source window:\n{window}\n\n\
Expected patches:\n{patches}",
        target_language = target_language,
        history = serde_json::Value::Array(history_json),
        window = window_json,
        patches = serde_json::Value::Array(expected_patches),
    )
}

fn cerebras_response_format() -> serde_json::Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "live_translate_patches",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "sourceStart": { "type": "integer" },
                                "sourceEnd": { "type": "integer" },
                                "state": { "type": "string", "enum": ["final", "draft"] },
                                "translation": { "type": "string" }
                            },
                            "required": ["sourceStart", "sourceEnd", "state", "translation"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["patches"],
                "additionalProperties": false
            }
        }
    })
}

fn compute_adaptive_translation_interval_ms(latency_ms: u64) -> u64 {
    (latency_ms + 250)
        .max(TRANSLATION_INTERVAL_MS)
        .min(TRANSLATION_INTERVAL_MAX_MS)
}

/// Unofficial Google Translate (GTX) fallback
pub fn translate_with_google_gtx(text: &str, target_lang: &str) -> Option<String> {
    let target_code = isolang::Language::from_name(target_lang)
        .and_then(|lang| lang.to_639_1())
        .map(|code| code.to_string())
        .unwrap_or_else(|| "en".to_string());

    let encoded_text = urlencoding::encode(text);
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
        target_code, encoded_text
    );

    if let Ok(resp) = UREQ_AGENT
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        && let Ok(json) = resp.into_body().read_json::<serde_json::Value>()
        && let Some(sentences) = json.get(0).and_then(|v| v.as_array())
    {
        let mut full_text = String::new();
        for sentence_node in sentences {
            if let Some(segment) = sentence_node.get(0).and_then(|s| s.as_str()) {
                full_text.push_str(segment);
            }
        }
        if !full_text.is_empty() {
            return Some(full_text);
        }
    }
    None
}
