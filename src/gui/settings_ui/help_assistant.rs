//! Help Assistant - Ask questions about SGT and get AI-powered answers.
//!
//! Uses verified product-help data with keyword search to retrieve only the
//! relevant guidance, then sends it to Gemini.

mod index;

use index::{ChunkEntry, get_help_index};

use std::sync::{Arc, LazyLock, Mutex, Weak};

static HELP_SHOW_LOCK: Mutex<()> = Mutex::new(());
static HELP_INPUT_OWNER: LazyLock<Mutex<Weak<()>>> = LazyLock::new(|| Mutex::new(Weak::new()));

const TOP_K: usize = 20;

const PRIMARY_MODEL: &str = crate::model_config::HELP_ASSISTANT_MODEL_CHAIN_IDS[0];
const FALLBACK_MODEL: &str = crate::model_config::HELP_ASSISTANT_MODEL_CHAIN_IDS[1];
const MAX_OUTPUT_TOKENS: u32 = 4096;

pub fn is_modal_open() -> bool {
    crate::overlay::text_input::is_active() && HELP_INPUT_OWNER.lock().unwrap().upgrade().is_some()
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

    let model = crate::model_config::get_model_by_id(model_id)
        .ok_or_else(|| format!("Unknown Help Assistant model: {model_id}"))?;
    if model.provider != "google" {
        return Err(format!(
            "Help Assistant model {model_id} is not a Gemini REST model"
        ));
    }

    // The key travels in a header so it never lands in proxy or URL logs.
    let url = crate::api::gemini_generate::gemini_content_url(&model.full_name, false);

    let system_prompt = r#"You are the SGT (Screen Goated Toolbox) Windows app help assistant. The user is asking from the Windows version — assume questions are about the Windows app unless they explicitly mention Android. Answer in a helpful, concise and easy to understand way in the question's language, no made up information, only true information. Go straight to the point. Do not mention "Based on the source code". If the answer needs to mention UI elements, use correct i18n locale terms matching the question's language. Format your response in Markdown."#;

    let user_message = format!(
        "{}\n\n---\nProduct Help Context:\n{}\n---\n\nUser Question: {}",
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
    if let Some(thinking) = crate::api::gemini_important_task_thinking_config(&model.full_name) {
        body["generationConfig"]["thinkingConfig"] = thinking;
    }

    let response = crate::api::client::UREQ_STREAM_AGENT
        .post(&url)
        .header("x-goog-api-key", gemini_api_key)
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
    // `text_input::show` can pump messages during warmup, so a second help
    // request on this thread must not wait for the first invocation.
    let Ok(_show) = HELP_SHOW_LOCK.try_lock() else {
        return;
    };
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
    let input_owner = Arc::new(());
    *HELP_INPUT_OWNER.lock().unwrap() = Arc::downgrade(&input_owner);
    let placeholder = crate::gui::locale::LocaleText::get(&ui_language)
        .global_settings
        .help_assistant_placeholder;

    crate::overlay::text_input::show(
        placeholder.to_string(),
        ui_language.clone(),
        String::new(),
        false,
        move |question, _hwnd, _session| {
            // The callback slot owns this marker. When another editor replaces
            // the slot, it cannot inherit help's shell-paste exclusion.
            let _input_owner = &input_owner;
            let question = question.trim().to_string();
            if question.is_empty() {
                return;
            }

            let gemini_key = gemini_api_key.clone();
            let lang = ui_language.clone();
            std::thread::spawn(move || {
                run_help_request(gemini_key, lang, question);
            });
        },
    );
}

fn run_help_request(gemini_key: String, ui_language: String, question: String) {
    let loading_msg = crate::gui::locale::LocaleText::get(&ui_language)
        .global_settings
        .help_assistant_finding_answer;

    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    if unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_err() {
        return;
    }
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _com = ComGuard;

    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, IsWindow, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let center_rect = crate::overlay::display_placement::primary_work_area().centered(600, 400);

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
            initial_text: loading_msg.to_string(),
            preset_id: None,
            is_chain_root: true,
            latency_trace_id: None,
        });

    if result_hwnd.is_invalid() {
        return;
    }
    let owner = crate::overlay::result::ChainCancelToken::new();
    let registered = {
        let mut states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&(result_hwnd.0 as isize)) {
            state.cancellation_token = Some(owner.clone());
            true
        } else {
            false
        }
    };
    if !registered {
        unsafe {
            let _ = DestroyWindow(result_hwnd);
        }
        return;
    }

    unsafe {
        let _ = ShowWindow(result_hwnd, SW_SHOW);
        let _ = SetForegroundWindow(result_hwnd);
    }

    let (response_tx, response_rx) = std::sync::mpsc::sync_channel(1);
    let request_owner = owner.clone();
    std::thread::spawn(move || {
        if request_owner.is_cancelled() {
            return;
        }
        // 1. Fetch + cache the help index
        let index = match get_help_index() {
            Ok(idx) => idx,
            Err(e) => {
                let _ = response_tx.send(format!("## Error\n\n{}", e));
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
        if request_owner.is_cancelled() {
            return;
        }
        let result = ask_gemini(&gemini_key, &question, &context, PRIMARY_MODEL).or_else(|error| {
            if request_owner.is_cancelled() {
                Err(error)
            } else {
                ask_gemini(&gemini_key, &question, &context, FALLBACK_MODEL)
            }
        });

        let response = match result {
            Ok(answer) => format!("### {}\n\n{}", question, answer),
            Err(e) => format!("## Error\n\n{}", e),
        };

        let _ = response_tx.send(response);
    });

    pump_result_messages(result_hwnd, &owner, &response_rx, |response| {
        if owns_result_window(result_hwnd, &owner) {
            crate::overlay::result::update_window_text(result_hwnd, response);
        }
    });
    unsafe {
        if IsWindow(Some(result_hwnd)).as_bool() && owns_result_window(result_hwnd, &owner) {
            let _ = DestroyWindow(result_hwnd);
        }
    }
}

fn pump_result_messages(
    hwnd: windows::Win32::Foundation::HWND,
    owner: &crate::overlay::result::ChainCancelToken,
    responses: &std::sync::mpsc::Receiver<String>,
    mut deliver: impl FnMut(&str),
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, IsWindow, KillTimer, MSG, SetTimer, TranslateMessage,
    };
    // A thread timer still wakes GetMessage if a sent close destroys the
    // window while GetMessage is waiting. A window timer would die with it.
    let timer = unsafe { SetTimer(None, 0, 100, None) };
    if timer == 0 {
        return;
    }
    unsafe {
        let mut msg = MSG::default();
        while !owner.is_cancelled() && GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if owner.is_cancelled() || !IsWindow(Some(hwnd)).as_bool() {
                break;
            }
            if let Ok(response) = responses.try_recv() {
                owner.run_if_active(|| deliver(&response));
            }
        }
        let _ = KillTimer(None, timer);
    }
}

fn owns_result_window(
    hwnd: windows::Win32::Foundation::HWND,
    owner: &std::sync::Arc<crate::overlay::result::ChainCancelToken>,
) -> bool {
    crate::overlay::result::state::WINDOW_STATES
        .lock()
        .unwrap()
        .get(&(hwnd.0 as isize))
        .and_then(|state| state.cancellation_token.as_ref())
        .is_some_and(|current| std::sync::Arc::ptr_eq(current, owner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_rest_url_carries_a_query_string_api_key() {
        fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan(&path, hits);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap_or_default();
                    let marker = ["?key", "={}"].concat();
                    if source.contains(&marker) {
                        hits.push(path.display().to_string());
                    }
                }
            }
        }
        let mut hits = Vec::new();
        scan(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        assert!(hits.is_empty(), "API keys belong in headers: {hits:?}");
    }

    #[test]
    fn cancelled_result_thread_wakes_and_rejects_late_response() {
        use std::time::Duration;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
        };
        use windows::core::w;

        let owner = crate::overlay::result::ChainCancelToken::new();
        let thread_owner = owner.clone();
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        let (delivered_tx, delivered_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .expect("create message-only test window");
            pump_result_messages(hwnd, &thread_owner, &response_rx, |response| {
                delivered_tx.send(response.to_string()).unwrap();
            });
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            finished_tx.send(()).unwrap();
        });
        response_tx.send("first".to_string()).unwrap();
        assert_eq!(
            delivered_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "first"
        );
        owner.cancel();
        let _ = response_tx.send("late".to_string());
        finished_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("message loop exits after cancellation");
        thread.join().unwrap();
        assert!(delivered_rx.try_recv().is_err());
    }

    #[test]
    fn closing_result_window_ends_message_loop_without_response() {
        use std::time::Duration;
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, HWND_MESSAGE, PostMessageW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
        };
        use windows::core::w;

        let (handle_tx, handle_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let (_response_tx, response_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .expect("create message-only test window");
            handle_tx.send(hwnd.0 as isize).unwrap();
            let owner = crate::overlay::result::ChainCancelToken::new();
            pump_result_messages(hwnd, &owner, &response_rx, |_| {
                panic!("no response expected")
            });
            finished_tx.send(()).unwrap();
        });
        let hwnd = HWND(handle_rx.recv_timeout(Duration::from_secs(5)).unwrap() as *mut _);
        unsafe {
            PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
        }
        finished_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("message loop exits after close");
        thread.join().unwrap();
    }

    #[test]
    fn model_chain_matches_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/mobile-shell/help-assistant.json"
        )))
        .expect("Help Assistant fixture parses");
        let model_chain = fixture["cases"]
            .as_array()
            .expect("fixture cases must be an array")
            .iter()
            .find(|case| case["name"] == "model_chain")
            .expect("model_chain case must exist");

        assert_eq!(model_chain["primary"], PRIMARY_MODEL);
        assert_eq!(model_chain["fallback"], FALLBACK_MODEL);
        assert_eq!(model_chain["max_output_tokens"], MAX_OUTPUT_TOKENS);
        for model_id in [PRIMARY_MODEL, FALLBACK_MODEL] {
            let api_model = crate::model_config::get_model_by_id(model_id)
                .unwrap()
                .full_name;
            assert_eq!(
                crate::api::gemini_important_task_thinking_config(&api_model).unwrap(),
                model_chain["thinking_config"][model_id]
            );
        }
    }
}
