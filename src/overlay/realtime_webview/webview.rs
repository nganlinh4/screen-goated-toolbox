//! The single WebView2 instance shared by both realtime cards.

use super::document::compositor_document;
use super::layout::{self, CardRole};
use super::state::*;
use crate::config::get_all_languages;
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_html::{RealtimeHtmlOptions, get_realtime_html};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HWND_TOPMOST, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
};
use wry::{Rect, WebViewBuilder};

pub fn create_realtime_webview(
    hwnd: HWND,
    audio_source: &str,
    current_language: &str,
    translation_model: &str,
    transcription_model: &str,
    font_size: u32,
) -> anyhow::Result<()> {
    let languages = get_all_languages();
    let (locale, is_dark) = {
        let app = crate::APP.lock().unwrap();
        (
            LocaleText::get(&app.config.ui_language),
            app.config.theme_mode.is_dark(),
        )
    };
    let transcription = get_realtime_html(RealtimeHtmlOptions {
        is_translation: false,
        audio_source,
        languages,
        current_language,
        translation_model,
        transcription_model,
        font_size,
        text: &locale,
        is_dark,
        compositor_role: Some(CardRole::Transcription.as_str()),
    });
    let translation = get_realtime_html(RealtimeHtmlOptions {
        is_translation: true,
        audio_source: "mic",
        languages,
        current_language,
        translation_model,
        transcription_model,
        font_size,
        text: &locale,
        is_dark,
        compositor_role: Some(CardRole::Translation.as_str()),
    });
    let html = compositor_document(&transcription, &translation);
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) } as u32;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) } as u32;
    let wrapper = HwndWrapper(hwnd);

    REALTIME_WEB_CONTEXT.with(|slot| {
        if slot.borrow().is_none() {
            let data_dir = crate::overlay::get_shared_webview_data_dir(Some("realtime-compositor"));
            *slot.borrow_mut() = Some(wry::WebContext::new(Some(data_dir)));
        }
    });

    let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
    crate::log_info!("[RealtimeCompositor] building singleton WebView");
    let webview = REALTIME_WEB_CONTEXT.with(|slot| {
        let mut context = slot.borrow_mut();
        let builder = WebViewBuilder::new_with_web_context(context.as_mut().unwrap());
        let builder = crate::overlay::html_components::font_manager::configure_webview(builder);
        let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
            .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));
        builder
            .with_bounds(Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(width, height)),
            })
            .with_url(&page_url)
            .with_transparent(true)
            .with_ipc_handler(move |request: wry::http::Request<String>| {
                super::ipc::handle(hwnd, request.body());
            })
            .build_as_child(&wrapper)
            .map_err(anyhow::Error::from)
    })?;
    REALTIME_WEBVIEW.with(|slot| {
        *slot.borrow_mut() = Some(webview);
    });
    crate::log_info!("[RealtimeCompositor] singleton WebView created");
    Ok(())
}

pub fn destroy_realtime_webview(_hwnd: HWND) {
    REALTIME_WEBVIEW.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

pub fn sync_session_settings_to_webview(reason: &str) {
    let config = super::controller::load_session_config();
    let payload = serde_json::json!({
        "audioSource": config.audio_source,
        "targetLanguage": config.target_language,
        "translationModel": config.translation_model,
        "transcriptionModel": config.transcription_model,
        "transcriptionLanguage": config.transcription_language.to_uppercase(),
        "fontSize": config.font_size,
    });
    crate::log_info!(
        "[RealtimeCompositor] sync settings reason={} transcription_model={} translation_model={}",
        reason,
        payload["transcriptionModel"].as_str().unwrap_or_default(),
        payload["translationModel"].as_str().unwrap_or_default()
    );
    let script = format!("if(window.updateSettings) window.updateSettings({payload});");
    run_all_cards_script(&script);
}

pub fn notify_card_settings(
    role: CardRole,
    source: &str,
    language: &str,
    translation_model: &str,
    transcription_model: &str,
    transcription_language: &str,
    font_size: u32,
) {
    let payload = serde_json::json!({
        "audioSource": source,
        "targetLanguage": language,
        "translationModel": translation_model,
        "transcriptionModel": transcription_model,
        "transcriptionLanguage": transcription_language.to_uppercase(),
        "fontSize": font_size,
    });
    run_card_script(
        role,
        &format!("if(window.updateSettings) window.updateSettings({payload});"),
    );
}

pub fn sync_visibility_to_webview() {
    use std::sync::atomic::Ordering;
    let mic = MIC_VISIBLE.load(Ordering::SeqCst);
    let translation = TRANS_VISIBLE.load(Ordering::SeqCst);
    run_all_cards_script(&format!(
        "if(window.setVisibility) window.setVisibility({mic}, {translation});"
    ));
}

pub fn update_card_text(role: CardRole, old_text: &str, new_text: &str) {
    use crate::overlay::utils::escape_js_single_quoted as escape_js;
    run_card_script(
        role,
        &format!(
            "window.updateText('{}', '{}');",
            escape_js(old_text),
            escape_js(new_text)
        ),
    );
}

pub fn clear_card_text(role: CardRole) {
    run_card_script(role, "if(window.clearText) window.clearText();");
}

pub fn update_theme() {
    let (is_dark, font_size) = crate::APP
        .lock()
        .map(|app| {
            (
                app.config.theme_mode.is_dark(),
                app.config.realtime_font_size,
            )
        })
        .unwrap_or((true, 24));
    for role in [CardRole::Transcription, CardRole::Translation] {
        let css = format!(
            "{}{}",
            crate::overlay::html_components::css_main::get(
                crate::overlay::utils::glow_color(role == CardRole::Translation),
                font_size,
                is_dark,
            ),
            crate::overlay::html_components::css_modals::get(is_dark)
        );
        let css = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
        run_card_script(
            role,
            &format!(
                "const style=document.getElementById('main-style');if(style)style.textContent={css};"
            ),
        );
    }
}

pub fn run_card_script(role: CardRole, script: &str) {
    let role = serde_json::to_string(role.as_str()).unwrap_or_else(|_| "\"transcription\"".into());
    let script = serde_json::to_string(script).unwrap_or_else(|_| "\"\"".into());
    evaluate(&format!("window.runRealtimeCardScript?.({role},{script});"));
}

pub fn run_all_cards_script(script: &str) {
    for role in [CardRole::Transcription, CardRole::Translation] {
        run_card_script(role, script);
    }
}

pub fn run_transcription_script(script: &str) {
    run_card_script(CardRole::Transcription, script);
}

pub fn sync_compositor_layout(hwnd: HWND) {
    let layout = layout::snapshot_for_renderer();
    layout::apply_native_region(hwnd);
    let payload = serde_json::to_string(&layout).unwrap_or_else(|_| "{}".into());
    evaluate(&format!("window.applyRealtimeLayout?.({payload});"));
    unsafe {
        if layout.transcription.visible || layout.translation.visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub fn resize_to_virtual_desktop(_hwnd: HWND) {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) } as u32;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) } as u32;
    REALTIME_WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(width, height)),
            });
        }
    });
}

fn evaluate(script: &str) {
    REALTIME_WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(script);
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn realtime_renderer_owns_exactly_one_webview_builder() {
        let source = include_str!("webview.rs");
        let constructor = ["WebViewBuilder", "new_with_web_context"].join("::");
        assert_eq!(source.matches(&constructor).count(), 1);
        assert!(source.contains("compositor_document(&transcription, &translation)"));
    }
}
