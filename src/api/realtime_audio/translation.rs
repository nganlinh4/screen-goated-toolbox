//! Translation loop for realtime audio

use isolang;
use std::io::BufRead;
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

use super::state::SharedRealtimeState;
use super::utils::{refresh_transcription_window, update_translation_text};
use super::{TRANSLATION_INTERVAL_MS, WM_MODEL_SWITCH};

/// Translation loop using the centralized realtime translation provider model.
pub fn run_translation_loop(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    translation_hwnd_send: crate::win_types::SendHwnd,
    state: SharedRealtimeState,
) {
    let translation_hwnd = translation_hwnd_send.0;
    let interval = Duration::from_millis(TRANSLATION_INTERVAL_MS);
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

        // Timeout check
        {
            let should_force = { state.lock().unwrap().should_force_commit_on_timeout() };
            if should_force && let Ok(mut s) = state.lock() {
                s.force_commit_all();
                let display = s.display_translation.clone();
                update_translation_text(translation_hwnd, &display);
                refresh_transcription_window();
            }
        }

        if last_run.elapsed() >= interval {
            if !crate::overlay::realtime_webview::TRANS_VISIBLE.load(Ordering::SeqCst) {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            let (chunk, has_finished, bytes_to_commit, is_unchanged) = {
                let s = state.lock().unwrap();
                if s.is_transcript_unchanged() {
                    (None, false, 0, true)
                } else {
                    match s.get_translation_chunk() {
                        Some((text, has_finished, len)) => (Some(text), has_finished, len, false),
                        None => (None, false, 0, true),
                    }
                }
            };

            if is_unchanged {
                last_run = Instant::now();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            if let Some(chunk) = chunk {
                {
                    let mut s = state.lock().unwrap();
                    s.update_last_processed_len();
                    s.start_new_translation();
                }

                let (gemini_key, cerebras_key, translation_model, history_messages) = {
                    let app = APP.lock().unwrap();
                    let gemini = app.config.gemini_api_key.clone();
                    let cerebras = app.config.cerebras_api_key.clone();
                    let model = app.config.realtime_translation_model.clone();
                    drop(app);
                    let history = if let Ok(s) = state.lock() {
                        s.get_history_messages(&target_language)
                    } else {
                        Vec::new()
                    };
                    (gemini, cerebras, model, history)
                };

                let current_model = translation_model.as_str();
                let mut primary_failed = false;

                if current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
                    if let Some(text) = translate_with_google_gtx(&chunk, &target_language) {
                        if let Ok(mut s) = state.lock() {
                            s.append_translation(&text);
                            if has_finished {
                                s.commit_current_translation();
                                s.advance_committed_pos(bytes_to_commit);
                            }
                            let display = s.display_translation.clone();
                            update_translation_text(translation_hwnd, &display);
                        }
                    } else {
                        primary_failed = true;
                    }
                } else {
                    let is_google =
                        current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GEMMA;
                    let (url, model_name, api_key) = if is_google {
                        (
                            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
                                .to_string(),
                            crate::model_config::realtime_translation_api_model(current_model)
                                .to_string(),
                            gemini_key.clone(),
                        )
                    } else {
                        (
                            "https://api.cerebras.ai/v1/chat/completions".to_string(),
                            crate::model_config::realtime_translation_api_model(current_model)
                                .to_string(),
                            cerebras_key.clone(),
                        )
                    };

                    let system_instruction = format!(
                        "You are a professional translator. Translate text to {} to append suitably to the context. Output ONLY the translation, nothing else.",
                        target_language
                    );

                    let mut messages: Vec<serde_json::Value> = Vec::new();
                    if is_google {
                        messages.extend(history_messages.clone());
                        messages.push(serde_json::json!({"role": "user", "content": format!("{}\n\nTranslate to {}:\n{}", system_instruction, target_language, chunk)}));
                    } else {
                        messages.push(
                            serde_json::json!({"role": "system", "content": system_instruction}),
                        );
                        messages.extend(history_messages.clone());
                        messages.push(serde_json::json!({"role": "user", "content": format!("Translate to {}:\n{}", target_language, chunk)}));
                    }

                    if !api_key.is_empty() {
                        let payload = serde_json::json!({"model": model_name, "messages": messages, "stream": true, "max_tokens": 512});
                        match UREQ_AGENT
                            .post(&url)
                            .header("Authorization", &format!("Bearer {}", api_key))
                            .header("Content-Type", "application/json")
                            .send_json(payload)
                        {
                            Ok(resp) => {
                                if !is_google
                                    && let Some(remaining) = resp
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
                                            crate::model_config::realtime_translation_api_model(
                                                current_model,
                                            )
                                            .to_string(),
                                            format!("{} / {}", remaining, limit),
                                        );
                                    }
                                }
                                let reader =
                                    std::io::BufReader::new(resp.into_body().into_reader());
                                let mut full_translation = String::new();
                                for line in reader.lines().map_while(Result::ok) {
                                    if stop_signal.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if json_str.trim() == "[DONE]" {
                                            break;
                                        }
                                        if let Ok(chunk_resp) =
                                            serde_json::from_str::<serde_json::Value>(json_str)
                                            && let Some(content) = chunk_resp
                                                .get("choices")
                                                .and_then(|c| c.as_array())
                                                .and_then(|a| a.first())
                                                .and_then(|f| f.get("delta"))
                                                .and_then(|d| d.get("content"))
                                                .and_then(|t| t.as_str())
                                        {
                                            full_translation.push_str(content);
                                            if let Ok(mut s) = state.lock() {
                                                s.append_translation(content);
                                                let display = s.display_translation.clone();
                                                update_translation_text(translation_hwnd, &display);
                                            }
                                        }
                                    }
                                }

                                if has_finished && let Ok(mut s) = state.lock() {
                                    if !full_translation.is_empty() {
                                        s.commit_current_translation();
                                    }
                                    s.advance_committed_pos(bytes_to_commit);
                                }
                            }
                            Err(_) => {
                                primary_failed = true;
                            }
                        }
                    } else {
                        primary_failed = true;
                    }
                }

                if primary_failed {
                    handle_fallback_translation(FallbackTranslationRequest {
                        chunk: &chunk,
                        target_language: &target_language,
                        current_model,
                        gemini_key: &gemini_key,
                        cerebras_key: &cerebras_key,
                        history_messages: &history_messages,
                        has_finished,
                        bytes_to_commit,
                        translation_hwnd,
                        state: &state,
                        stop_signal: &stop_signal,
                    });
                }
            }
            last_run = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

struct FallbackTranslationRequest<'a> {
    chunk: &'a str,
    target_language: &'a str,
    current_model: &'a str,
    gemini_key: &'a str,
    cerebras_key: &'a str,
    history_messages: &'a [serde_json::Value],
    has_finished: bool,
    bytes_to_commit: usize,
    translation_hwnd: HWND,
    state: &'a SharedRealtimeState,
    stop_signal: &'a Arc<AtomicBool>,
}

fn handle_fallback_translation(request: FallbackTranslationRequest<'_>) {
    let FallbackTranslationRequest {
        chunk,
        target_language,
        current_model,
        gemini_key,
        cerebras_key,
        history_messages,
        has_finished,
        bytes_to_commit,
        translation_hwnd,
        state,
        stop_signal,
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

    if alt_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        if let Some(text) = translate_with_google_gtx(chunk, target_language)
            && let Ok(mut s) = state.lock()
        {
            s.append_translation(&text);
            if has_finished {
                s.commit_current_translation();
                s.advance_committed_pos(bytes_to_commit);
            }
            let display = s.display_translation.clone();
            update_translation_text(translation_hwnd, &display);
        }
    } else {
        let alt_is_google = alt_model == crate::model_config::REALTIME_TRANSLATION_MODEL_GEMMA;
        let (alt_url, alt_model_name, alt_key) = if alt_is_google {
            (
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
                    .to_string(),
                crate::model_config::realtime_translation_api_model(alt_model).to_string(),
                gemini_key.to_string(),
            )
        } else {
            (
                "https://api.cerebras.ai/v1/chat/completions".to_string(),
                crate::model_config::realtime_translation_api_model(alt_model).to_string(),
                cerebras_key.to_string(),
            )
        };

        let system_instruction = format!(
            "You are a professional translator. Translate text to {} to append suitably to the context. Output ONLY the translation, nothing else.",
            target_language
        );
        let mut messages: Vec<serde_json::Value> = Vec::new();
        if alt_is_google {
            messages.extend(history_messages.iter().cloned());
            messages.push(serde_json::json!({"role": "user", "content": format!("{}\n\nTranslate to {}:\n{}", system_instruction, target_language, chunk)}));
        } else {
            messages.push(serde_json::json!({"role": "system", "content": system_instruction}));
            messages.extend(history_messages.iter().cloned());
            messages.push(serde_json::json!({"role": "user", "content": format!("Translate to {}:\n{}", target_language, chunk)}));
        }

        if !alt_key.is_empty() {
            let payload = serde_json::json!({"model": alt_model_name, "messages": messages, "stream": true, "max_tokens": 512});
            if let Ok(resp) = UREQ_AGENT
                .post(&alt_url)
                .header("Authorization", &format!("Bearer {}", alt_key))
                .header("Content-Type", "application/json")
                .send_json(payload)
            {
                let reader = std::io::BufReader::new(resp.into_body().into_reader());
                let mut full_translation = String::new();
                for line in reader.lines().map_while(Result::ok) {
                    if stop_signal.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            break;
                        }
                        if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str)
                            && let Some(content) = chunk_resp
                                .get("choices")
                                .and_then(|c| c.as_array())
                                .and_then(|a| a.first())
                                .and_then(|f| f.get("delta"))
                                .and_then(|d| d.get("content"))
                                .and_then(|t| t.as_str())
                        {
                            full_translation.push_str(content);
                            if let Ok(mut s) = state.lock() {
                                s.append_translation(content);
                                let display = s.display_translation.clone();
                                update_translation_text(translation_hwnd, &display);
                            }
                        }
                    }
                }

                if has_finished && let Ok(mut s) = state.lock() {
                    if !full_translation.is_empty() {
                        s.commit_current_translation();
                    }
                    s.advance_committed_pos(bytes_to_commit);
                }
            }
        }
    }
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
