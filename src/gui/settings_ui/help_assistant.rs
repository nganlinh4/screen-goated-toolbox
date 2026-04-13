//! Help Assistant - Ask questions about SGT and get AI-powered answers.
//!
//! Uses a pre-built chunk index (help-index.json) with keyword search to
//! retrieve only the relevant source files, then sends them to Gemini.

use lazy_static::lazy_static;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

static HELP_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);

const HELP_INDEX_URL: &str =
    "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/help-index.json";
const TOP_K: usize = 20;

lazy_static! {
    static ref HELP_ASSISTANT_AGENT: ureq::Agent = {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(900)))
            .build();
        config.into()
    };
    /// Cached help index — fetched once, reused across queries.
    static ref HELP_INDEX_CACHE: Mutex<Option<Vec<ChunkEntry>>> = Mutex::new(None);
}

#[derive(Clone)]
struct ChunkEntry {
    path: String,
    text: String,
}

const PRIMARY_MODEL: &str = "gemini-3.1-flash-lite-preview";
const FALLBACK_MODEL: &str = "gemma-4-26b-a4b-it";
const MAX_OUTPUT_TOKENS: u32 = 4096;

pub fn is_modal_open() -> bool {
    HELP_INPUT_ACTIVE.load(Ordering::SeqCst)
}

/// Fetch and cache the help index from GitHub.
fn get_help_index() -> Result<Vec<ChunkEntry>, String> {
    {
        let cache = HELP_INDEX_CACHE.lock().unwrap();
        if let Some(ref idx) = *cache {
            return Ok(idx.clone());
        }
    }

    let body = HELP_ASSISTANT_AGENT
        .get(HELP_INDEX_URL)
        .call()
        .map_err(|e| format!("Failed to fetch help index: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read help index: {}", e))?;

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&body).map_err(|e| format!("Invalid help index JSON: {}", e))?;

    let entries: Vec<ChunkEntry> = raw
        .into_iter()
        .filter_map(|v| {
            Some(ChunkEntry {
                path: v["path"].as_str()?.to_string(),
                text: v["text"].as_str()?.to_string(),
            })
        })
        .collect();

    let mut cache = HELP_INDEX_CACHE.lock().unwrap();
    *cache = Some(entries.clone());
    Ok(entries)
}

/// Simple keyword/BM25-style search: score each chunk by how many query
/// terms appear in its path + text. Returns top-K chunks.
fn search_chunks<'a>(index: &'a [ChunkEntry], question: &str) -> Vec<&'a ChunkEntry> {
    let terms: Vec<String> = question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect();

    if terms.is_empty() {
        return index.iter().take(TOP_K).collect();
    }

    let mut scored: Vec<(usize, f64)> = index
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let haystack = format!("{}\n{}", chunk.path, chunk.text).to_lowercase();
            let mut score = 0.0;
            for term in &terms {
                let count = haystack.matches(term.as_str()).count();
                if count > 0 {
                    // BM25-ish: diminishing returns for repeated matches
                    score += 1.0 + (count as f64).ln();
                }
            }
            // Boost path matches (file name is a strong signal)
            let path_lower = chunk.path.to_lowercase();
            for term in &terms {
                if path_lower.contains(term.as_str()) {
                    score += 3.0;
                }
            }
            (i, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .iter()
        .take(TOP_K)
        .filter(|(_, s)| *s > 0.0)
        .map(|(i, _)| &index[*i])
        .collect()
}

fn ask_gemini(
    gemini_api_key: &str,
    question: &str,
    context: &str,
    model_id: &str,
) -> Result<String, String> {
    if gemini_api_key.trim().is_empty() {
        return Err("Gemini API key not configured. Please set it in Global Settings.".to_string());
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model_id, gemini_api_key
    );

    let system_prompt = r#"You are the SGT (Screen Goated Toolbox) Windows app help assistant. The user is asking from the Windows version — assume questions are about the Windows app unless they explicitly mention Android. Answer in a helpful, concise and easy to understand way in the question's language, no made up information, only true information. Go straight to the point. Do not mention "Based on the source code". If the answer needs to mention UI elements, use correct i18n locale terms matching the question's language. Format your response in Markdown."#;

    let user_message = format!(
        "{}\n\n---\nSource Code Context:\n{}\n---\n\nUser Question: {}",
        system_prompt, context, question
    );

    let mut body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": user_message
            }]
        }],
        "generationConfig": {
            "maxOutputTokens": MAX_OUTPUT_TOKENS,
            "temperature": 0.7
        }
    });
    if let Some(thinking) = crate::api::gemini_thinking_config(model_id) {
        body["generationConfig"]["thinkingConfig"] = thinking;
    }

    let response = HELP_ASSISTANT_AGENT
        .post(&url)
        .header("Content-Type", "application/json")
        .send(&body.to_string())
        .map_err(|e| format!("API request failed: {}", e))?;

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let parts = json["candidates"][0]["content"]["parts"]
        .as_array()
        .ok_or_else(|| "Failed to extract response text".to_string())?;
    // Filter out thought parts (thinking model output) — only keep content
    let text: String = parts
        .iter()
        .filter(|p| !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false))
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("");
    if text.trim().is_empty() {
        return Err("Failed to extract response text".to_string());
    }
    Ok(text.trim().to_string())
}

pub fn show_help_input() {
    HELP_INPUT_ACTIVE.store(true, Ordering::SeqCst);

    let (gemini_api_key, ui_language) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.gemini_api_key.clone(),
            app.config.ui_language.clone(),
        )
    };

    show_help_question_input(gemini_api_key, ui_language);
}

fn show_help_question_input(gemini_api_key: String, ui_language: String) {
    let submitted = Arc::new(AtomicBool::new(false));
    let submit_state = submitted.clone();

    let placeholder = match ui_language.as_str() {
        "vi" => "Hỏi gì về SGT? (VD: Làm sao để dịch vùng màn hình?)",
        "ko" => "SGT에 대해 무엇을 물어볼까요?",
        _ => "Ask anything about SGT (e.g., How do I translate a screen region?)",
    };

    crate::overlay::text_input::show(
        placeholder.to_string(),
        ui_language.clone(),
        String::new(),
        false,
        move |question, _hwnd| {
            let question = question.trim().to_string();
            if question.is_empty() {
                HELP_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }

            submit_state.store(true, Ordering::SeqCst);

            let gemini_key = gemini_api_key.clone();
            let lang = ui_language.clone();
            std::thread::spawn(move || {
                run_help_request(gemini_key, lang, question);
            });
        },
    );

    std::thread::spawn(move || {
        while crate::overlay::text_input::is_active() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        if !submitted.load(Ordering::SeqCst) {
            HELP_INPUT_ACTIVE.store(false, Ordering::SeqCst);
        }
    });
}

fn run_help_request(gemini_key: String, ui_language: String, question: String) {
    let loading_msg = match ui_language.as_str() {
        "vi" => "⏳ Đang tìm câu trả lời...",
        "ko" => "⏳ 답변을 찾는 중...",
        _ => "⏳ Finding the answer...",
    };

    unsafe {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, GetSystemMetrics, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
        SetForegroundWindow, ShowWindow, TranslateMessage,
    };

    let (screen_w, screen_h) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };

    let center_rect = windows::Win32::Foundation::RECT {
        left: screen_w / 2 - 300,
        top: screen_h / 2 - 200,
        right: screen_w / 2 + 300,
        bottom: screen_h / 2 + 200,
    };

    let result_hwnd =
        crate::overlay::result::create_result_window(crate::overlay::result::ResultWindowParams {
            target_rect: center_rect,
            win_type: crate::overlay::result::WindowType::Primary,
            context: crate::overlay::result::RefineContext::None,
            model_id: PRIMARY_MODEL.to_string(),
            provider: "google".to_string(),
            streaming_enabled: false,
            start_editing: false,
            preset_prompt: "Ask SGT".to_string(),
            custom_bg_color: crate::overlay::result::get_chain_color(0),
            render_mode: "markdown",
            initial_text: loading_msg.to_string(),
            preset_id: None,
            is_chain_root: true,
        });

    unsafe {
        let _ = ShowWindow(result_hwnd, SW_SHOW);
        let _ = SetForegroundWindow(result_hwnd);
    }

    let api_hwnd_val = result_hwnd.0 as isize;
    std::thread::spawn(move || {
        let api_hwnd = windows::Win32::Foundation::HWND(api_hwnd_val as *mut std::ffi::c_void);

        // 1. Fetch + cache the help index
        let index = match get_help_index() {
            Ok(idx) => idx,
            Err(e) => {
                crate::overlay::result::update_window_text(
                    api_hwnd,
                    &format!("## ❌ Error\n\n{}", e),
                );
                return;
            }
        };

        // 2. Keyword search for relevant chunks
        let top_chunks = search_chunks(&index, &question);
        let context: String = top_chunks
            .iter()
            .map(|c| format!("=== {} ===\n{}", c.path, c.text))
            .collect::<Vec<_>>()
            .join("\n\n");

        // 3. Ask Gemini — try primary model, fall back on error
        let result = ask_gemini(&gemini_key, &question, &context, PRIMARY_MODEL)
            .or_else(|_| ask_gemini(&gemini_key, &question, &context, FALLBACK_MODEL));

        let response = match result {
            Ok(answer) => format!("### {}\n\n{}", question, answer),
            Err(e) => format!("## ❌ Error\n\n{}", e),
        };

        crate::overlay::result::update_window_text(api_hwnd, &response);
    });

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    HELP_INPUT_ACTIVE.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_constants_are_set() {
        assert_eq!(PRIMARY_MODEL, "gemini-3.1-flash-lite-preview");
        assert_eq!(FALLBACK_MODEL, "gemma-4-26b-a4b-it");
    }
}
