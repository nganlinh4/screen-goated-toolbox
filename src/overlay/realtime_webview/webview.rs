//! WebView creation and IPC handling for realtime overlay

use super::controller;
use super::state::*;
use crate::APP;
use crate::api::realtime_audio::{WM_COPY_TEXT, WM_EXEC_SCRIPT};
use crate::api::realtime_audio::{WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};
use crate::config::get_all_languages;
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_html::{RealtimeHtmlOptions, get_realtime_html};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebViewBuilder};

pub fn sync_session_settings_to_webviews(reason: &str) {
    let config = controller::load_session_config();
    let payload = serde_json::json!({
        "audioSource": config.audio_source,
        "targetLanguage": config.target_language,
        "translationModel": config.translation_model,
        "transcriptionModel": config.transcription_model,
        "transcriptionLanguage": config.transcription_language.to_uppercase(),
        "fontSize": config.font_size,
    });
    let script = format!(
        "if(window.updateSettings) window.updateSettings({});",
        payload
    );

    unsafe {
        let realtime_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        let translation_hwnd = std::ptr::addr_of!(TRANSLATION_HWND).read();
        crate::log_info!(
            "[Realtime] syncing session settings to WebViews reason={} transcription_model={} translation_model={} target_language={}",
            reason,
            payload["transcriptionModel"].as_str().unwrap_or_default(),
            payload["translationModel"].as_str().unwrap_or_default(),
            payload["targetLanguage"].as_str().unwrap_or_default()
        );

        REALTIME_WEBVIEWS.with(|wvs| {
            let wvs = wvs.borrow();
            for hwnd in [realtime_hwnd, translation_hwnd] {
                if hwnd.is_invalid() {
                    continue;
                }
                let hwnd_key = hwnd.0 as isize;
                if let Some(webview) = wvs.get(&hwnd_key) {
                    if let Err(error) = webview.evaluate_script(&script) {
                        crate::log_info!(
                            "[Realtime] failed to sync settings to hwnd={:?}: {:?}",
                            hwnd,
                            error
                        );
                    }
                } else {
                    crate::log_info!("[Realtime] no WebView for settings sync hwnd={:?}", hwnd);
                }
            }
        });
    }
}

pub fn create_realtime_webview(
    hwnd: HWND,
    is_translation: bool,
    audio_source: &str,
    current_language: &str,
    translation_model: &str,
    transcription_model: &str,
    font_size: u32,
) {
    let hwnd_key = hwnd.0 as isize;
    crate::log_info!("[Realtime] Creating WebView for HWND: {:?}", hwnd);

    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }

    // Use full language list from isolang crate
    let languages = get_all_languages();

    // Fetch locale text
    let locale_text = {
        let app = APP.lock().unwrap();
        let lang = app.config.ui_language.clone();
        LocaleText::get(&lang)
    };

    let is_dark = if let Ok(app) = crate::APP.lock() {
        match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        }
    } else {
        true
    };

    let html = get_realtime_html(RealtimeHtmlOptions {
        is_translation,
        audio_source,
        languages,
        current_language,
        translation_model,
        transcription_model,
        font_size,
        text: &locale_text,
        is_dark,
    });
    let wrapper = HwndWrapper(hwnd);

    // Capture hwnd for the IPC handler closure
    let hwnd_for_ipc = hwnd;

    REALTIME_WEB_CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
            *ctx.borrow_mut() = Some(wry::WebContext::new(Some(shared_data_dir)));
        }
    });

    let result = {
        // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
        let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
        crate::log_info!(
            "[Realtime] Acquired init lock. Building for HWND: {:?}...",
            hwnd
        );

        let build_res = REALTIME_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };
            let builder = crate::overlay::html_components::font_manager::configure_webview(builder);

            // Store HTML in font server and get URL for same-origin font loading
            let page_url =
                crate::overlay::html_components::font_manager::store_html_page(html.clone())
                    .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

            let builder = builder
                .with_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        (rect.right - rect.left) as u32,
                        (rect.bottom - rect.top) as u32,
                    )),
                })
                .with_url(&page_url)
                .with_transparent(false)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    let body = msg.body();
                    if body == "startDrag" {
                        // Initiate window drag directly
                        unsafe {
                            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                            SendMessageW(
                                hwnd_for_ipc,
                                WM_NCLBUTTONDOWN,
                                Some(WPARAM(HTCAPTION as usize)),
                                Some(LPARAM(0)),
                            );
                        }
                    } else if body.starts_with("toggleMic:") {
                        // Toggle transcription window visibility directly
                        let visible = &body[10..] == "1";
                        MIC_VISIBLE.store(visible, Ordering::SeqCst);
                        unsafe {
                            if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                                if visible {
                                    update_webview_theme(REALTIME_HWND);
                                }
                                let _ = ShowWindow(
                                    REALTIME_HWND,
                                    if visible { SW_SHOW } else { SW_HIDE },
                                );
                            }
                            // Sync to other webview
                            sync_visibility_to_webviews();

                            // If both windows are now off, hide and reset state (but keep windows alive)
                            if !MIC_VISIBLE.load(Ordering::SeqCst)
                                && !TRANS_VISIBLE.load(Ordering::SeqCst)
                            {
                                REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
                                REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
                                crate::api::tts::TTS_MANAGER.stop();
                                IS_ACTIVE = false;
                            } else if visible {
                                // Force update since we suppressed them while hidden
                                let _ = PostMessageW(
                                    Some(REALTIME_HWND),
                                    WM_REALTIME_UPDATE,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        }
                    } else if body.starts_with("toggleTrans:") {
                        // Toggle translation window visibility directly
                        let visible = &body[12..] == "1";
                        TRANS_VISIBLE.store(visible, Ordering::SeqCst);

                        // Stop TTS when translation window is hidden
                        if !visible {
                            crate::api::tts::TTS_MANAGER.stop();
                        }

                        unsafe {
                            if !std::ptr::addr_of!(TRANSLATION_HWND).read().is_invalid() {
                                if visible {
                                    update_webview_theme(TRANSLATION_HWND);
                                }
                                let _ = ShowWindow(
                                    TRANSLATION_HWND,
                                    if visible { SW_SHOW } else { SW_HIDE },
                                );
                            }
                            // Sync to other webview
                            sync_visibility_to_webviews();

                            // If both windows are now off, hide and reset state (but keep windows alive)
                            if !MIC_VISIBLE.load(Ordering::SeqCst)
                                && !TRANS_VISIBLE.load(Ordering::SeqCst)
                            {
                                REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
                                REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
                                crate::api::tts::TTS_MANAGER.stop();
                                IS_ACTIVE = false;
                            } else if visible {
                                // Force update since we suppressed them while hidden
                                let _ = PostMessageW(
                                    Some(TRANSLATION_HWND),
                                    WM_TRANSLATION_UPDATE,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        }
                    } else if body == "startGroupDrag" {
                        // Start group drag - nothing special needed, just mark drag started
                        // The actual movement is handled by groupDragMove
                    } else if let Some(coords) = body.strip_prefix("groupDragMove:") {
                        // Move both windows together by delta
                        if let Some((dx_str, dy_str)) = coords.split_once(',')
                            && let (Ok(dx), Ok(dy)) = (dx_str.parse::<i32>(), dy_str.parse::<i32>())
                        {
                            unsafe {
                                // Move realtime window
                                if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                                    let mut rect = RECT::default();
                                    let _ = GetWindowRect(REALTIME_HWND, &mut rect);
                                    let _ = SetWindowPos(
                                        REALTIME_HWND,
                                        None,
                                        rect.left + dx,
                                        rect.top + dy,
                                        0,
                                        0,
                                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                                    );
                                }

                                // Move translation window
                                if !std::ptr::addr_of!(TRANSLATION_HWND).read().is_invalid() {
                                    let mut rect = RECT::default();
                                    let _ = GetWindowRect(TRANSLATION_HWND, &mut rect);
                                    let _ = SetWindowPos(
                                        TRANSLATION_HWND,
                                        None,
                                        rect.left + dx,
                                        rect.top + dy,
                                        0,
                                        0,
                                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                                    );
                                }
                            }
                        }
                    } else if let Some(text) = body.strip_prefix("copyText:") {
                        // Copy text to clipboard via UI thread
                        let boxed = Box::new(text.to_string());
                        let ptr = Box::into_raw(boxed);
                        unsafe {
                            let _ = PostMessageW(
                                Some(hwnd_for_ipc),
                                WM_COPY_TEXT,
                                WPARAM(0),
                                LPARAM(ptr as isize),
                            );
                        }
                    } else if body == "close" {
                        unsafe {
                            let _ =
                                PostMessageW(Some(hwnd_for_ipc), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    } else if body == "saveResize" {
                        unsafe {
                            let mut rect = RECT::default();
                            let _ = GetWindowRect(hwnd_for_ipc, &mut rect);
                            let w = rect.right - rect.left;
                            let h = rect.bottom - rect.top;

                            let mut app = APP.lock().unwrap();
                            if hwnd_for_ipc == REALTIME_HWND {
                                app.config.realtime_transcription_size = (w, h);
                            } else {
                                app.config.realtime_translation_size = (w, h);
                            }
                            crate::config::save_config(&app.config);
                        }
                    } else if let Some(size) = body.strip_prefix("fontSize:") {
                        // Font size change - store for future use
                        if let Ok(size) = size.parse::<u32>() {
                            controller::set_font_size(size);
                        }
                    } else if let Some(source) = body.strip_prefix("audioSource:") {
                        controller::set_audio_source(source);
                        sync_session_settings_to_webviews("audio-source-ipc");
                    } else if let Some(lang) = body.strip_prefix("language:") {
                        controller::set_target_language(lang);
                        sync_session_settings_to_webviews("target-language-ipc");
                    } else if let Some(model) = body.strip_prefix("translationModel:") {
                        controller::set_translation_model(model);
                        sync_session_settings_to_webviews("translation-model-ipc");
                    } else if let Some(model) = body.strip_prefix("transcriptionModel:") {
                        controller::set_transcription_model(model);
                        sync_session_settings_to_webviews("transcription-model-ipc");
                    } else if let Some(lang_code) = body.strip_prefix("transcriptionLanguage:") {
                        controller::set_transcription_language(lang_code);
                        sync_session_settings_to_webviews("transcription-language-ipc");
                    } else if let Some(coords) = body.strip_prefix("resize:") {
                        // Resize window by delta
                        if let Some((dx_str, dy_str)) = coords.split_once(',')
                            && let (Ok(dx), Ok(dy)) = (dx_str.parse::<i32>(), dy_str.parse::<i32>())
                        {
                            unsafe {
                                let mut rect = RECT::default();
                                let _ = GetWindowRect(hwnd_for_ipc, &mut rect);
                                let new_width = (rect.right - rect.left + dx).max(200);
                                let new_height = (rect.bottom - rect.top + dy).max(100);
                                let _ = SetWindowPos(
                                    hwnd_for_ipc,
                                    None,
                                    rect.left,
                                    rect.top,
                                    new_width,
                                    new_height,
                                    SWP_NOZORDER | SWP_NOACTIVATE,
                                );
                            }
                        }
                    } else if body.starts_with("ttsEnabled:") {
                        // TTS toggle for realtime translations
                        let requested_enabled = &body[11..] == "1";
                        controller::set_tts_enabled(requested_enabled);
                        if controller::load_session_config().transcription_model
                            != "gemini-live-s2s"
                            && requested_enabled
                            && controller::load_session_config().audio_source == "device"
                        {
                            let script = "if(window.setTtsEnabled) window.setTtsEnabled(false);";
                            let script_ptr = Box::into_raw(Box::new(script.to_string()));
                            unsafe {
                                let _ = PostMessageW(
                                    Some(hwnd_for_ipc),
                                    WM_EXEC_SCRIPT,
                                    WPARAM(0),
                                    LPARAM(script_ptr as isize),
                                );
                            }
                        }
                    } else if let Some(speed) = body.strip_prefix("ttsSpeed:") {
                        // TTS playback speed adjustment (50-200, where 100 = 1.0x)
                        if let Ok(speed) = speed.parse::<u32>() {
                            controller::set_tts_speed(speed);
                        }
                    } else if body.starts_with("ttsAutoSpeed:") {
                        // TTS auto-speed toggle
                        let enabled = &body[13..] == "1";
                        controller::set_tts_auto_speed(enabled);
                    } else if let Some(vol) = body.strip_prefix("ttsVolume:") {
                        // TTS output volume (0-100)
                        if let Ok(vol) = vol.parse::<u32>() {
                            controller::set_tts_volume(vol);
                        }
                    } else if body == "cancelDownload" {
                        // Cancel Parakeet download and revert to Gemini
                        controller::cancel_download();
                    }
                });
            builder.build_as_child(&wrapper)
        });
        crate::log_info!(
            "[Realtime] Build finished. Releasing lock. Status: {}",
            if build_res.is_ok() { "OK" } else { "ERR" }
        );
        build_res
    };

    if let Ok(webview) = result {
        crate::log_info!("[Realtime] WebView success for HWND: {:?}", hwnd);
        REALTIME_WEBVIEWS.with(|wvs| {
            wvs.borrow_mut().insert(hwnd_key, webview);
        });
    } else if let Err(e) = result {
        crate::log_info!(
            "[Realtime] WebView FAILED for HWND: {:?}, Error: {:?}",
            hwnd,
            e
        );
    }
}

pub fn destroy_realtime_webview(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    REALTIME_WEBVIEWS.with(|wvs| {
        wvs.borrow_mut().remove(&hwnd_key);
    });
}

/// Sync visibility toggle state to all webviews
pub fn sync_visibility_to_webviews() {
    let mic_vis = MIC_VISIBLE.load(Ordering::SeqCst);
    let trans_vis = TRANS_VISIBLE.load(Ordering::SeqCst);
    let script = format!(
        "if(window.setVisibility) window.setVisibility({}, {});",
        mic_vis, trans_vis
    );

    REALTIME_WEBVIEWS.with(|wvs| {
        for webview in wvs.borrow().values() {
            let _ = webview.evaluate_script(&script);
        }
    });
}

pub fn update_webview_text(hwnd: HWND, old_text: &str, new_text: &str) {
    let hwnd_key = hwnd.0 as isize;

    // Escape the text for JavaScript
    fn escape_js(text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "")
    }

    let escaped_old = escape_js(old_text);
    let escaped_new = escape_js(new_text);

    let script = format!("window.updateText('{}', '{}');", escaped_old, escaped_new);

    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&script);
        }
    });
}

/// Clear/reset the WebView text to initial "Đang chờ nói..." state
pub fn clear_webview_text(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    let script = "if(window.clearText) window.clearText();";

    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(script);
        }
    });
}

pub fn update_webview_theme(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let is_dark = if let Ok(app) = crate::APP.lock() {
        match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        }
    } else {
        true
    };

    let font_size = if let Ok(app) = crate::APP.lock() {
        app.config.realtime_font_size
    } else {
        24
    };

    // Determine glow color based on whether this is a translation window
    let is_translation = unsafe { hwnd == TRANSLATION_HWND };
    let glow_color = if is_translation { "#ff9633" } else { "#00c8ff" };

    let css = format!(
        "{}{}",
        crate::overlay::html_components::css_main::get(glow_color, font_size, is_dark),
        crate::overlay::html_components::css_modals::get(is_dark)
    );
    let css_escaped = css.replace("`", "\\`");

    let script = format!(
        r#"
        if (document.getElementById('main-style')) {{
            document.getElementById('main-style').innerHTML = `{}`;
        }}
        "#,
        css_escaped
    );

    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&script);
        }
    });
}
