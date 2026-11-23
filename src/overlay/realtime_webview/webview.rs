//! The single child-process WebView2 instance shared by both realtime cards.

use super::document::compositor_document;
use super::layout::{self, CardRole};
use super::protocol::{
    CardSettings, CardText, DownloadState, HostCommand, RealtimeScene, TextEdit,
};
use super::state::*;
use crate::config::get_all_languages;
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_html::{RealtimeHtmlOptions, get_realtime_html};
use std::cell::RefCell;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HWND_TOPMOST, IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
};
use wry::{Rect, WebViewBuilder};

#[path = "script_queue.rs"]
mod script_queue;
thread_local! { static SCRIPTS: RefCell<script_queue::ScriptQueue> = RefCell::new(script_queue::ScriptQueue::default()); }
static SCRIPT_CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);
static SCRIPT_STARTED_MS: AtomicU64 = AtomicU64::new(0);

static PAINT_REQUEST: AtomicU64 = AtomicU64::new(0);
static LAYOUT_ACTIVE: AtomicBool = AtomicBool::new(false);

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
            *slot.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                crate::overlay::webview_runtime::Profile::RealtimeCompositor,
            ));
        }
    });

    let gate = crate::overlay::webview_init::acquire("realtime-compositor");
    let webview = REALTIME_WEB_CONTEXT.with(|slot| {
        let mut context = slot.borrow_mut();
        let builder = WebViewBuilder::new_with_web_context(context.as_mut().unwrap());
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
    });
    gate.finish(webview.is_ok());
    let webview = webview?;
    super::webview_failure::attach(hwnd, &webview);
    REALTIME_WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));
    Ok(())
}

pub fn destroy_realtime_webview(_hwnd: HWND) {
    REALTIME_WEBVIEW.with(|slot| *slot.borrow_mut() = None);
}

pub(super) fn apply_command(hwnd: HWND, command: &HostCommand) {
    match command {
        HostCommand::Snapshot { scene } => apply_snapshot(hwnd, scene.as_ref()),
        HostCommand::Layout { layout: snapshot } => {
            layout::replace(*snapshot);
            sync_compositor_layout(hwnd);
        }
        HostCommand::Text { role, text } => update_card_text(*role, text),
        HostCommand::TextEdit { role, edit } => edit_card_text(*role, edit),
        HostCommand::Settings { settings } => update_settings(settings),
        HostCommand::Tts { enabled, speed } => update_tts(*enabled, *speed),
        HostCommand::Volume { rms } => run_card_script(
            CardRole::Transcription,
            &format!("if(window.updateVolume)window.updateVolume({rms});"),
        ),
        HostCommand::TranslationModel { model } => {
            let model = serde_json::to_string(model).unwrap_or_else(|_| "\"\"".into());
            run_card_script(
                CardRole::Translation,
                &format!("if(window.switchModel)window.switchModel({model});"),
            );
        }
        HostCommand::Download { state } => update_download(state),
        HostCommand::Theme { is_dark, font_size } => update_theme(*is_dark, *font_size),
        HostCommand::Script { role, script } => match role {
            Some(role) => run_card_script(*role, script),
            None => run_all_cards_script(script),
        },
        HostCommand::Shutdown => {}
    }
}

fn apply_snapshot(hwnd: HWND, scene: &RealtimeScene) {
    let Some(changed) = super::ipc::install_session(scene.session_id) else {
        return;
    };
    if changed {
        LAYOUT_ACTIVE.store(false, Ordering::SeqCst);
        PAINT_REQUEST.fetch_add(1, Ordering::SeqCst);
        super::text_input_focus::end(hwnd);
        layout::set_interaction_active(hwnd, false);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    layout::replace(scene.layout);
    update_settings(&scene.settings);
    update_card_text(CardRole::Transcription, &scene.transcription);
    update_card_text(CardRole::Translation, &scene.translation);
    update_tts(scene.tts_enabled, scene.tts_speed);
    update_theme(scene.is_dark, scene.settings.font_size);
    update_download(&scene.download);
    run_card_script(
        CardRole::Transcription,
        &format!("if(window.updateVolume)window.updateVolume({});", scene.rms),
    );
    apply_layout(hwnd, scene.active, Some(scene.session_id));
}

fn update_settings(settings: &CardSettings) {
    let payload = |audio_source: &str| {
        serde_json::json!({
            "audioSource": audio_source,
            "targetLanguage": settings.target_language,
            "translationModel": settings.translation_model,
            "transcriptionModel": settings.transcription_model,
            "transcriptionLanguage": settings.transcription_language,
            "fontSize": settings.font_size,
        })
    };
    run_card_script(
        CardRole::Transcription,
        &format!(
            "if(window.updateSettings)window.updateSettings({});",
            payload(&settings.audio_source)
        ),
    );
    run_card_script(
        CardRole::Translation,
        &format!(
            "if(window.updateSettings)window.updateSettings({});",
            payload("mic")
        ),
    );
}

fn update_card_text(role: CardRole, text: &CardText) {
    let committed = serde_json::to_string(&text.committed).unwrap_or_else(|_| "\"\"".into());
    let draft = serde_json::to_string(&text.draft).unwrap_or_else(|_| "\"\"".into());
    run_card_script(
        role,
        &format!("window.setRealtimeText({committed},{draft});"),
    );
}

fn edit_card_text(role: CardRole, edit: &TextEdit) {
    let append = serde_json::to_string(&edit.append).unwrap_or_else(|_| "\"\"".into());
    let draft = serde_json::to_string(&edit.draft).unwrap_or_else(|_| "\"\"".into());
    run_card_script(
        role,
        &format!(
            "window.editRealtimeText({},{append},{draft});",
            edit.remove_utf16
        ),
    );
}

fn update_tts(enabled: bool, speed: u32) {
    run_all_cards_script(&format!(
        "if(window.setTtsEnabled)window.setTtsEnabled({enabled});if(window.updateTtsSpeed)window.updateTtsSpeed({speed});"
    ));
}

fn update_download(state: &DownloadState) {
    if !state.active {
        run_card_script(
            CardRole::Transcription,
            "if(window.hideDownloadModal)window.hideDownloadModal();",
        );
        return;
    }
    let title = serde_json::to_string(&state.title).unwrap_or_else(|_| "\"\"".into());
    let message = serde_json::to_string(&state.message).unwrap_or_else(|_| "\"\"".into());
    run_card_script(
        CardRole::Transcription,
        &format!(
            "if(window.showDownloadModal)window.showDownloadModal({title},{message},{});",
            state.progress
        ),
    );
}

fn update_theme(is_dark: bool, font_size: u32) {
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

fn run_card_script(role: CardRole, script: &str) {
    let role = serde_json::to_string(role.as_str()).unwrap_or_else(|_| "\"transcription\"".into());
    let script = serde_json::to_string(script).unwrap_or_else(|_| "\"\"".into());
    evaluate(&format!("window.runRealtimeCardScript?.({role},{script});"));
}

pub(super) fn focus_card_text_input(role: CardRole, session_id: u64) {
    REALTIME_WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.focus();
        }
    });
    let role = serde_json::to_string(role.as_str()).unwrap_or_else(|_| "\"transcription\"".into());
    evaluate_owned(
        &format!(
            "window.runRealtimeCardScript?.({role},\"window.focusCustomVocabularyInput?.();\");"
        ),
        Some(session_id),
    );
}

fn run_all_cards_script(script: &str) {
    for role in [CardRole::Transcription, CardRole::Translation] {
        run_card_script(role, script);
    }
}

pub(super) fn sync_compositor_layout(hwnd: HWND) {
    apply_layout(hwnd, true, None);
}

fn apply_layout(hwnd: HWND, active: bool, session_id: Option<u64>) {
    let snapshot = layout::snapshot_for_renderer();
    layout::apply_native_region(hwnd);
    let payload = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".into());
    LAYOUT_ACTIVE.store(active, Ordering::SeqCst);
    let already_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let paint_request = PAINT_REQUEST.fetch_add(1, Ordering::SeqCst) + 1;
    let session = session_id
        .map(|id| format!("\"{id}\""))
        .unwrap_or_else(|| "null".to_string());
    evaluate(&format!(
        "window.applyRealtimeLayout?.({payload},{},{});",
        if active && !already_visible {
            paint_request
        } else {
            0
        },
        session
    ));
    if active && already_visible {
        show_for_layout(hwnd);
    } else if !active {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub(super) fn reveal_after_paint(hwnd: HWND, request: u64) {
    if LAYOUT_ACTIVE.load(Ordering::SeqCst) && request == PAINT_REQUEST.load(Ordering::SeqCst) {
        show_for_layout(hwnd);
    }
}

fn show_for_layout(hwnd: HWND) {
    let snapshot = layout::snapshot();
    unsafe {
        if snapshot.transcription.visible || snapshot.translation.visible {
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
            super::text_input_focus::end(hwnd);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub(super) fn resize_to_virtual_desktop(_hwnd: HWND) {
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
    evaluate_owned(script, None);
}

fn evaluate_owned(script: &str, session_id: Option<u64>) {
    if !SCRIPTS.with(|queue| queue.borrow_mut().push(script.to_owned(), session_id)) {
        crate::crash_report::exit(1, "realtime-script-queue-overflow");
    }
    pump_script();
}

fn pump_script() {
    let script = loop {
        let Some(script) = SCRIPTS.with(|queue| queue.borrow_mut().next()) else {
            return;
        };
        if script.admitted(super::ipc::current_session()) {
            break script.source;
        }
        SCRIPTS.with(|queue| {
            queue.borrow_mut().complete();
        });
    };
    SCRIPT_STARTED_MS.store(script_clock_ms(), Ordering::SeqCst);
    let script =
        format!("(()=>{{try{{{script}\n;return true;}}catch(_error){{return false;}}}})()");
    REALTIME_WEBVIEW.with(|slot| {
        let slot = slot.borrow();
        let Some(webview) = slot.as_ref() else {
            crate::crash_report::exit(1, "realtime-script-webview-missing");
        };
        if webview
            .evaluate_script_with_callback(&script, |result| {
                if result != "true" {
                    crate::crash_report::exit(1, "realtime-script-failed");
                }
                super::child::post_script_complete();
            })
            .is_err()
        {
            crate::crash_report::exit(1, "realtime-script-submit-failed");
        }
    });
}

pub(super) fn script_complete() {
    if !SCRIPTS.with(|queue| queue.borrow_mut().complete()) {
        crate::crash_report::exit(1, "realtime-script-unexpected-completion");
    }
    SCRIPT_STARTED_MS.store(0, Ordering::SeqCst);
    pump_script();
}

pub(super) fn script_timed_out() -> bool {
    let started = SCRIPT_STARTED_MS.load(Ordering::SeqCst);
    started != 0
        && SCRIPT_CLOCK.elapsed()
            > Duration::from_millis(started.saturating_sub(1))
                .saturating_add(Duration::from_secs(10))
}

fn script_clock_ms() -> u64 {
    (SCRIPT_CLOCK
        .elapsed()
        .as_millis()
        .min((u64::MAX - 1) as u128) as u64)
        + 1
}

pub(super) fn reset_script_timeout_after_suspend() {
    let started = SCRIPT_STARTED_MS.load(Ordering::SeqCst);
    if started != 0 {
        let _ = SCRIPT_STARTED_MS.compare_exchange(
            started,
            script_clock_ms(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
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

    #[test]
    fn vocabulary_editor_requests_scoped_keyboard_focus() {
        let source = include_str!("../html_components/js_main/vocabulary_editor.js");
        assert!(source.contains("realtimePostMessage('textInputStart')"));
        assert!(source.contains("realtimePostMessage('textInputEnd')"));
        assert!(source.contains("window.focusCustomVocabularyInput"));
    }
}
