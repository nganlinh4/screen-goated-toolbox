#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::Foundation::HWND;
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::Graphics::Gdi::InvalidateRect;

#[cfg(not(feature = "recorder-worker"))]
use crate::overlay::result::state::WINDOW_STATES;

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn set_tts_request(hwnd: isize, request_id: u64) {
    if hwnd == 0 {
        return;
    }
    if let Some(state) = WINDOW_STATES.lock().unwrap().get_mut(&hwnd) {
        state.tts_request_id = request_id;
        state.tts_loading = true;
    }
}

/// Clear the TTS loading state for a window and trigger repaint
#[cfg(not(feature = "recorder-worker"))]
pub fn clear_tts_loading_state(hwnd: isize, request_id: u64) {
    #[cfg(not(feature = "recorder-worker"))]
    {
        // Skip window state management for hwnd=0 (no associated window, e.g. auto-speak without overlay)
        if hwnd == 0 {
            return;
        }

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states
                .get_mut(&hwnd)
                .filter(|state| state.tts_request_id == request_id)
            {
                state.tts_loading = false;
            }
        }

        // Trigger repaint to update button appearance
        unsafe {
            let _ = InvalidateRect(Some(HWND(hwnd as *mut std::ffi::c_void)), None, false);
        }

        // Notify the unified result controls that state has changed.
        crate::overlay::result::scene_compositor::sync_all_controls();
    }
}

/// Clear TTS state completely when speech ends
#[cfg(not(feature = "recorder-worker"))]
pub fn clear_tts_state(hwnd: isize, request_id: u64) {
    #[cfg(not(feature = "recorder-worker"))]
    {
        // Skip window state management for hwnd=0 (no associated window, e.g. auto-speak without overlay)
        if hwnd == 0 {
            return;
        }

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states
                .get_mut(&hwnd)
                .filter(|state| state.tts_request_id == request_id)
            {
                state.tts_loading = false;
                state.tts_request_id = 0;
            }
        }

        // Trigger repaint to update button appearance
        unsafe {
            let _ = InvalidateRect(Some(HWND(hwnd as *mut std::ffi::c_void)), None, false);
        }

        // Notify the unified result controls that state has changed.
        crate::overlay::result::scene_compositor::sync_all_controls();
    }
}

/// Detect language of text and get matching TTS instruction from config conditions
pub fn get_language_instruction_for_text(
    text: &str,
    conditions: &[crate::config::TtsLanguageCondition],
) -> Option<String> {
    if conditions.is_empty() {
        return None;
    }
    let detected_code = crate::lang_detect::detect_language(text)?;
    get_language_instruction_for_code(&detected_code, conditions)
}

/// Get matching TTS instruction from a known ISO 639-3 language code.
pub fn get_language_instruction_for_code(
    detected_code: &str,
    conditions: &[crate::config::TtsLanguageCondition],
) -> Option<String> {
    // Find matching condition
    for condition in conditions {
        if condition.language_code.eq_ignore_ascii_case(detected_code) {
            return Some(condition.instruction.clone());
        }
    }
    None
}

pub fn resolve_edge_voice_for_language(
    profile: &crate::api::tts::types::TtsRequestProfile,
    detected_code: Option<&str>,
) -> (String, String, String, &'static str) {
    let edge_language_code = detected_code
        .and_then(isolang::Language::from_639_3)
        .and_then(|language| language.to_639_1())
        .unwrap_or("en")
        .to_string();

    if let Some(config) = profile.edge_settings.voice_configs.iter().find(|config| {
        config
            .language_code
            .eq_ignore_ascii_case(&edge_language_code)
    }) {
        return (
            config.voice_name.clone(),
            edge_language_code,
            config.voice_name.clone(),
            "profile-config",
        );
    }

    if let Some(config) = crate::config::TtsPlaygroundSettings::default()
        .edge_settings
        .voice_configs
        .into_iter()
        .find(|config| {
            config
                .language_code
                .eq_ignore_ascii_case(&edge_language_code)
        })
    {
        return (
            config.voice_name.clone(),
            edge_language_code,
            config.voice_name.clone(),
            "default-language-config",
        );
    }

    (
        crate::config::TtsPlaygroundSettings::default().edge_voice,
        edge_language_code,
        String::new(),
        "default-fallback",
    )
}
