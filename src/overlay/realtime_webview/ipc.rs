//! IPC actions emitted by cards inside the unified realtime compositor.

use super::controller;
use super::layout::{self, CardRole};
use super::state::*;
use serde::Deserialize;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

#[derive(Deserialize)]
struct IpcEnvelope {
    role: String,
    body: String,
    #[serde(default = "unit_scale")]
    scale: f64,
}

fn unit_scale() -> f64 {
    1.0
}

pub(super) fn handle(hwnd: HWND, raw: &str) {
    let Ok(message) = serde_json::from_str::<IpcEnvelope>(raw) else {
        crate::log_info!(
            "[RealtimeCompositor] rejected malformed IPC bytes={}",
            raw.len()
        );
        return;
    };
    if message.role == "compositor" && message.body == "ready" {
        super::manager::on_compositor_ready(hwnd);
        return;
    }
    let Some(role) = CardRole::parse(&message.role) else {
        return;
    };
    handle_card_message(hwnd, role, &message.body, message.scale);
}

fn handle_card_message(hwnd: HWND, role: CardRole, body: &str, scale: f64) {
    if body == "interactionStart" {
        layout::set_interaction_active(hwnd, true);
    } else if body == "interactionEnd" {
        layout::set_interaction_active(hwnd, false);
    } else if let Some((dx, dy)) = parse_delta(body, "cardDragMove:", scale) {
        layout::move_card(role, dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if let Some((dx, dy)) = parse_delta(body, "groupDragMove:", scale) {
        layout::move_group(dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if let Some((dx, dy)) = parse_delta(body, "resize:", scale) {
        layout::resize_card(role, dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if body == "saveResize" {
        save_size(role);
    } else if let Some(visible) = parse_toggle(body, "toggleMic:") {
        set_card_visibility(hwnd, CardRole::Transcription, visible);
    } else if let Some(visible) = parse_toggle(body, "toggleTrans:") {
        set_card_visibility(hwnd, CardRole::Translation, visible);
    } else if let Some(text) = body.strip_prefix("copyText:") {
        crate::overlay::utils::copy_to_clipboard(text, hwnd);
    } else if body == "close" {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    } else if let Some(size) = body.strip_prefix("fontSize:") {
        if let Ok(size) = size.parse::<u32>() {
            controller::set_font_size(size);
        }
    } else if let Some(source) = body.strip_prefix("audioSource:") {
        controller::set_audio_source(source);
        super::webview::sync_session_settings_to_webview("audio-source-ipc");
    } else if let Some(language) = body.strip_prefix("language:") {
        controller::set_target_language(language);
        super::webview::sync_session_settings_to_webview("target-language-ipc");
    } else if let Some(model) = body.strip_prefix("translationModel:") {
        controller::set_translation_model(model);
        super::webview::sync_session_settings_to_webview("translation-model-ipc");
    } else if let Some(model) = body.strip_prefix("transcriptionModel:") {
        controller::set_transcription_model(model);
        super::webview::sync_session_settings_to_webview("transcription-model-ipc");
    } else if let Some(language) = body.strip_prefix("transcriptionLanguage:") {
        controller::set_transcription_language(language);
        super::webview::sync_session_settings_to_webview("transcription-language-ipc");
    } else if let Some(enabled) = parse_toggle(body, "ttsEnabled:") {
        handle_tts_enabled(role, enabled);
    } else if let Some(speed) = body.strip_prefix("ttsSpeed:") {
        if let Ok(speed) = speed.parse::<u32>() {
            controller::set_tts_speed(speed);
        }
    } else if let Some(enabled) = parse_toggle(body, "ttsAutoSpeed:") {
        controller::set_tts_auto_speed(enabled);
    } else if let Some(volume) = body.strip_prefix("ttsVolume:") {
        if let Ok(volume) = volume.parse::<u32>() {
            controller::set_tts_volume(volume);
        }
    } else if body == "cancelDownload" {
        controller::cancel_download();
    }
}

fn set_card_visibility(hwnd: HWND, role: CardRole, visible: bool) {
    match role {
        CardRole::Transcription => MIC_VISIBLE.store(visible, Ordering::SeqCst),
        CardRole::Translation => {
            TRANS_VISIBLE.store(visible, Ordering::SeqCst);
            if !visible {
                crate::api::tts::TTS_MANAGER.stop();
            }
        }
    }
    layout::set_visible(role, visible);
    super::webview::sync_visibility_to_webview();
    super::webview::sync_compositor_layout(hwnd);

    if !MIC_VISIBLE.load(Ordering::SeqCst) && !TRANS_VISIBLE.load(Ordering::SeqCst) {
        REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
        REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
        crate::api::tts::TTS_MANAGER.stop();
        unsafe { IS_ACTIVE = false };
    } else if visible {
        let message = match role {
            CardRole::Transcription => crate::api::realtime_audio::WM_REALTIME_UPDATE,
            CardRole::Translation => crate::api::realtime_audio::WM_TRANSLATION_UPDATE,
        };
        unsafe {
            let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
        }
    }
}

fn handle_tts_enabled(role: CardRole, requested_enabled: bool) {
    controller::set_tts_enabled(requested_enabled);
    let config = controller::load_session_config();
    if !crate::model_config::is_gemini_live_s2s_model_id(&config.transcription_model)
        && requested_enabled
        && config.audio_source == "device"
    {
        super::webview::run_card_script(
            role,
            "if(window.setTtsEnabled) window.setTtsEnabled(false);",
        );
    }
}

fn save_size(role: CardRole) {
    let (width, height) = layout::card_size(role);
    if let Ok(mut app) = crate::APP.lock() {
        match role {
            CardRole::Transcription => app.config.realtime_transcription_size = (width, height),
            CardRole::Translation => app.config.realtime_translation_size = (width, height),
        }
        crate::config::save_config(&app.config);
    }
}

fn parse_delta(body: &str, prefix: &str, scale: f64) -> Option<(i32, i32)> {
    let (x, y) = body.strip_prefix(prefix)?.split_once(',')?;
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    Some((
        (x.parse::<f64>().ok()? * scale).round() as i32,
        (y.parse::<f64>().ok()? * scale).round() as i32,
    ))
}

fn parse_toggle(body: &str, prefix: &str) -> Option<bool> {
    match body.strip_prefix(prefix)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_delta, parse_toggle};

    #[test]
    fn compositor_deltas_are_converted_from_css_to_physical_pixels() {
        assert_eq!(parse_delta("resize:4,-2", "resize:", 1.5), Some((6, -3)));
        assert_eq!(parse_toggle("toggleMic:1", "toggleMic:"), Some(true));
        assert_eq!(parse_toggle("toggleMic:no", "toggleMic:"), None);
    }
}
