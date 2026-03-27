//! Help Assistant - Ask questions about SGT and get AI-powered answers.

use crate::overlay::preset_wheel::WheelOption;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static HELP_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);

lazy_static! {
    static ref HELP_ASSISTANT_AGENT: ureq::Agent = {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(900)))
            .build();
        config.into()
    };
}

#[derive(Clone, Copy)]
enum HelpBucket {
    ScreenRecorder,
    Android,
    Rest,
}

#[derive(Clone, Copy)]
enum HelpMode {
    Quick,
    Detailed,
}

impl HelpBucket {
    fn raw_url(self) -> &'static str {
        match self {
            Self::ScreenRecorder => {
                "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-screen-recorder.xml"
            }
            Self::Android => {
                "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-android.xml"
            }
            Self::Rest => {
                "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-rest.xml"
            }
        }
    }

    fn prompt_guide(self, locale: &crate::gui::locale::LocaleText) -> &'static str {
        match self {
            Self::ScreenRecorder => locale.help_assistant_screen_record_option,
            Self::Android => locale.help_assistant_android_option,
            Self::Rest => locale.help_assistant_rest_option,
        }
    }

    fn placeholder(self, ui_language: &str) -> &'static str {
        match self {
            Self::ScreenRecorder => match ui_language {
                "vi" => {
                    "Hỏi về SGT Record / Quay màn hình (VD: Làm sao để xuất MP4 hoặc chọn cửa sổ?)"
                }
                "ko" => "SGT Record / 화면 녹화에 대해 물어보세요. 예: MP4로 내보내려면?",
                _ => {
                    "Ask about SGT Record / Screen Record (e.g., How do I export MP4 or pick a window?)"
                }
            },
            Self::Android => match ui_language {
                "vi" => {
                    "Hỏi về SGT Android (VD: Làm sao dùng bubble, preset hay overlay trong app?)"
                }
                "ko" => {
                    "SGT Android에 대해 물어보세요. 예: 버블, 프리셋, 오버레이는 어떻게 쓰나요?"
                }
                _ => {
                    "Ask about SGT Android (e.g., How do I use the bubble, presets, or overlays in the app?)"
                }
            },
            Self::Rest => match ui_language {
                "vi" => "Hỏi gì về SGT? (VD: Làm sao để dịch vùng màn hình?)",
                "ko" => "SGT의 다른 기능에 대해 무엇을 물어볼까요?",
                _ => "Ask anything else about SGT (e.g., How do I translate a screen region?)",
            },
        }
    }

    fn loading_message(self, ui_language: &str) -> &'static str {
        match self {
            Self::ScreenRecorder => match ui_language {
                "vi" => "⏳ Đang tìm câu trả lời về SGT Record / Quay màn hình...",
                "ko" => "⏳ SGT Record / 화면 녹화 답변을 찾는 중...",
                _ => "⏳ Finding the answer for SGT Record / Screen Record...",
            },
            Self::Android => match ui_language {
                "vi" => "⏳ Đang tìm câu trả lời về SGT Android...",
                "ko" => "⏳ SGT Android 답변을 찾는 중...",
                _ => "⏳ Finding the answer for SGT Android...",
            },
            Self::Rest => match ui_language {
                "vi" => "⏳ Đang gọi cho tác giả nganlinh4 ... Kkk đùa thôi, đợi tí nha",
                "ko" => "⏳ 작가 nganlinh4에게 전화 중... ㅋㅋ 농담이고, 잠깐만 기다려",
                _ => "⏳ Calling author nganlinh4 ... Kkk joke, wait a bit",
            },
        }
    }

    fn preset_prompt(self) -> &'static str {
        match self {
            Self::ScreenRecorder => "Ask SGT Record",
            Self::Android => "Ask SGT Android",
            Self::Rest => "Ask SGT",
        }
    }

    fn response_icon(self) -> &'static str {
        match self {
            Self::ScreenRecorder => "🎬",
            Self::Android => "📱",
            Self::Rest => "❓",
        }
    }
}

impl HelpMode {
    fn label(self, locale: &crate::gui::locale::LocaleText) -> &'static str {
        match self {
            Self::Quick => locale.help_assistant_quick_option,
            Self::Detailed => locale.help_assistant_detailed_option,
        }
    }

    fn model_id(self) -> &'static str {
        match self {
            Self::Quick => "gemini-3.1-flash-lite-preview",
            Self::Detailed => "gemini-3-flash-preview",
        }
    }

    fn max_output_tokens(self) -> u32 {
        match self {
            Self::Quick => 2048,
            Self::Detailed => 4096,
        }
    }

    fn prompt_instruction(self) -> &'static str {
        match self {
            Self::Quick => {
                "Keep the answer short, direct, and practical unless the user clearly asks for more detail."
            }
            Self::Detailed => {
                "Give a more detailed answer with clear steps, practical context, and useful caveats when needed."
            }
        }
    }
}

pub fn is_modal_open() -> bool {
    HELP_INPUT_ACTIVE.load(Ordering::SeqCst)
}

fn fetch_repomix_xml(bucket: HelpBucket) -> Result<String, String> {
    match HELP_ASSISTANT_AGENT.get(bucket.raw_url()).call() {
        Ok(response) => response
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response: {}", e)),
        Err(e) => Err(format!("Failed to fetch XML: {}", e)),
    }
}

fn ask_gemini(
    gemini_api_key: &str,
    question: &str,
    context_xml: &str,
    mode: HelpMode,
) -> Result<String, String> {
    if gemini_api_key.trim().is_empty() {
        return Err("Gemini API key not configured. Please set it in Global Settings.".to_string());
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        mode.model_id(),
        gemini_api_key
    );

    let system_prompt = r#"
Answer the user in a helpful, concise and easy to understand way in the question's language, no made up infomation, only the true infomation. Go straight to the point, dont mention thing like "Based on the source code", if answer needs to mention the UI, be sure to use correct i18n locale terms matching the question's language. Format your response in Markdown."#;

    let user_message = format!(
        "{} {}\n\n---\nSource Code Context:\n{}\n---\n\nUser Question: {}",
        system_prompt,
        mode.prompt_instruction(),
        context_xml,
        question
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": user_message
            }]
        }],
        "generationConfig": {
            "maxOutputTokens": mode.max_output_tokens(),
            "temperature": 0.7
        }
    });

    let response = HELP_ASSISTANT_AGENT
        .post(&url)
        .header("Content-Type", "application/json")
        .send(&body.to_string())
        .map_err(|e| format!("API request failed: {}", e))?;

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Failed to extract response text".to_string())
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

    let Some(bucket) = choose_help_bucket(&ui_language) else {
        HELP_INPUT_ACTIVE.store(false, Ordering::SeqCst);
        return;
    };

    let Some(mode) = choose_help_mode(&ui_language) else {
        HELP_INPUT_ACTIVE.store(false, Ordering::SeqCst);
        return;
    };

    show_help_question_input(gemini_api_key, ui_language, bucket, mode);
}

fn choose_help_bucket(ui_language: &str) -> Option<HelpBucket> {
    let locale = crate::gui::locale::LocaleText::get(ui_language);
    let options = [
        WheelOption::new(0, locale.help_assistant_screen_record_option),
        WheelOption::new(1, locale.help_assistant_android_option),
        WheelOption::new(2, locale.help_assistant_rest_option),
    ];

    let mut center_pos = windows::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut center_pos);
    }

    match crate::overlay::preset_wheel::show_option_wheel(&options, center_pos) {
        Some(0) => Some(HelpBucket::ScreenRecorder),
        Some(1) => Some(HelpBucket::Android),
        Some(2) => Some(HelpBucket::Rest),
        _ => None,
    }
}

fn choose_help_mode(ui_language: &str) -> Option<HelpMode> {
    let locale = crate::gui::locale::LocaleText::get(ui_language);
    let options = [
        WheelOption::new(0, HelpMode::Quick.label(&locale)),
        WheelOption::new(1, HelpMode::Detailed.label(&locale)),
    ];

    let mut center_pos = windows::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut center_pos);
    }

    match crate::overlay::preset_wheel::show_option_wheel(&options, center_pos) {
        Some(0) => Some(HelpMode::Quick),
        Some(1) => Some(HelpMode::Detailed),
        _ => None,
    }
}

fn show_help_question_input(
    gemini_api_key: String,
    ui_language: String,
    bucket: HelpBucket,
    mode: HelpMode,
) {
    let submitted = Arc::new(AtomicBool::new(false));
    let submit_state = submitted.clone();

    crate::overlay::text_input::show(
        bucket.placeholder(&ui_language).to_string(),
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
                run_help_request(gemini_key, lang, bucket, mode, question);
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

fn run_help_request(
    gemini_key: String,
    ui_language: String,
    bucket: HelpBucket,
    mode: HelpMode,
    question: String,
) {
    let loading_msg = bucket.loading_message(&ui_language);

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
            model_id: mode.model_id().to_string(),
            provider: "google".to_string(),
            streaming_enabled: false,
            start_editing: false,
            preset_prompt: bucket.preset_prompt().to_string(),
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
        let locale = crate::gui::locale::LocaleText::get(&ui_language);
        let result = match fetch_repomix_xml(bucket) {
            Ok(xml) => ask_gemini(&gemini_key, &question, &xml, mode),
            Err(e) => Err(format!("Failed to fetch context: {}", e)),
        };

        let response = match result {
            Ok(answer) => format!(
                "## {} {}\n\n### {}\n\n{}",
                bucket.response_icon(),
                bucket.prompt_guide(&locale),
                question,
                answer
            ),
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
    use super::{HelpBucket, HelpMode};

    #[test]
    fn help_buckets_match_three_way_repomix_split() {
        assert_eq!(
            HelpBucket::ScreenRecorder.raw_url(),
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-screen-recorder.xml"
        );
        assert_eq!(
            HelpBucket::Android.raw_url(),
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-android.xml"
        );
        assert_eq!(
            HelpBucket::Rest.raw_url(),
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-rest.xml"
        );
    }

    #[test]
    fn android_bucket_uses_dedicated_prompt_and_icon() {
        assert_eq!(HelpBucket::Android.preset_prompt(), "Ask SGT Android");
        assert_eq!(HelpBucket::Android.response_icon(), "📱");
    }

    #[test]
    fn help_modes_map_to_expected_models() {
        assert_eq!(HelpMode::Quick.model_id(), "gemini-3.1-flash-lite-preview");
        assert_eq!(HelpMode::Detailed.model_id(), "gemini-3-flash-preview");
        assert_eq!(HelpMode::Quick.max_output_tokens(), 2048);
        assert_eq!(HelpMode::Detailed.max_output_tokens(), 4096);
    }
}
