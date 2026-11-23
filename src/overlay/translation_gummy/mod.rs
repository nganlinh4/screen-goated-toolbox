mod assets;
mod ipc;
mod runtime;
mod startup;
mod state;
mod volume;
mod window;

#[cfg(test)]
mod contract_tests;

use std::sync::Once;
use std::sync::atomic::AtomicBool;

use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{TranslationGummySettings, save_config};
use crate::gui::locale::LocaleText;

pub use runtime::{TranslationGummyConnectionState, TranslationGummyTranscriptItem};

pub(super) const WM_APP_SHOW: u32 = WM_USER + 321;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 322;
pub(super) const WM_APP_METER: u32 = WM_USER + 323;

pub static REQUEST_OPEN_TTS_SETTINGS: AtomicBool = AtomicBool::new(false);
pub static REQUEST_DISMISS_SPLASH: AtomicBool = AtomicBool::new(false);
pub(super) static REGISTER_CLASS: Once = Once::new();
pub(super) static WINDOW: crate::overlay::window_slot::WindowSlot =
    crate::overlay::window_slot::WindowSlot::new();

thread_local! {
    pub(super) static WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { std::cell::RefCell::new(None) };
}

pub fn show_translation_gummy() {
    let capability = crate::runtime_support::require_webview2(
        crate::runtime_support::WebViewFeature::TranslationGummy,
    );
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }

    window::show();
}

pub fn toggle_translation_gummy() {
    if window::close_if_open() {
        return;
    }
    show_translation_gummy();
}

pub fn update_settings() {
    let old_tts = runtime::current_gemini_tts_settings();
    state::refresh_from_config();
    let new_tts = runtime::current_gemini_tts_settings();
    // Only restart if voice or model actually changed
    if old_tts != new_tts {
        let is_running = state::snapshot().is_running;
        if is_running {
            runtime::stop_session();
            let applied = current_settings();
            if applied.is_valid() {
                start_if_possible(applied);
            }
        }
    }
    state::request_sync();
}

pub(super) fn publish_connection(
    connection_state: TranslationGummyConnectionState,
    is_running: bool,
    last_error: Option<String>,
) {
    state::publish_connection(connection_state, is_running, last_error);
    state::request_sync();
}

pub(super) fn publish_error(
    connection_state: TranslationGummyConnectionState,
    error: String,
    is_running: bool,
) {
    state::publish_error(connection_state, error, is_running);
    state::request_sync();
}

pub(super) fn auto_start_if_possible() {
    let applied = current_settings();
    if applied.is_valid() {
        start_if_possible(applied);
    } else {
        publish_connection(TranslationGummyConnectionState::NotConfigured, false, None);
    }
}

pub(super) fn start_if_possible(settings: TranslationGummySettings) {
    let locale = LocaleText::get(&current_ui_language());
    let api_key_missing = crate::APP
        .lock()
        .map(|app| app.config.gemini_api_key.trim().is_empty())
        .unwrap_or(true);
    if !startup::admit_start(
        !api_key_missing,
        locale.translation_gummy.translation_gummy_api_key_required,
        |effect| match effect {
            startup::StartEffect::LocalError(message) => {
                publish_error(TranslationGummyConnectionState::Error, message, false);
            }
            startup::StartEffect::GlobalNotice(message) => {
                crate::overlay::auto_copy_badge::show_error_notification(&message);
            }
        },
    ) {
        return;
    }

    if let Some(hwnd) = WINDOW.hwnd() {
        runtime::start_session(hwnd.0 as isize, settings);
    }
}

fn settings_for_apply(
    draft: &TranslationGummySettings,
    current: &TranslationGummySettings,
) -> TranslationGummySettings {
    let mut applied = draft.clone();
    applied.hotkeys = current.hotkeys.clone();
    applied.guide_seen = current.guide_seen;
    applied
}

pub(super) fn apply_draft() {
    let (mut draft, can_apply) = state::with_state(|ui| {
        ui.normalize();
        (ui.draft.clone(), ui.draft.is_valid())
    });

    if !can_apply {
        publish_connection(TranslationGummyConnectionState::NotConfigured, false, None);
        return;
    }

    runtime::stop_session();

    {
        let mut app = crate::APP.lock().unwrap();
        draft = settings_for_apply(&draft, &app.config.translation_gummy);
        app.config.translation_gummy = draft.clone();
        save_config(&app.config);
    }

    state::insert_session_separator();
    runtime::request_persist();
    state::with_state(|ui| {
        ui.applied = draft.clone();
        ui.draft = draft.clone();
        ui.last_error = None;
        ui.hotkey_error = None;
        ui.audio_level = 0.0;
        ui.normalize();
    });

    state::request_sync();
    start_if_possible(draft);
}

pub(super) fn toggle_run() {
    let snapshot = state::snapshot();
    if snapshot.is_running {
        runtime::stop_session();
        publish_connection(TranslationGummyConnectionState::Stopped, false, None);
        return;
    }
    if snapshot.applied.is_valid() {
        start_if_possible(snapshot.applied);
    } else {
        publish_connection(TranslationGummyConnectionState::NotConfigured, false, None);
    }
}

pub(super) fn current_settings() -> TranslationGummySettings {
    crate::APP
        .lock()
        .map(|app| app.config.translation_gummy.clone().normalized())
        .unwrap_or_default()
}

pub(super) fn current_ui_language() -> String {
    crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string())
}

pub(super) fn connection_key(connection_state: TranslationGummyConnectionState) -> &'static str {
    match connection_state {
        TranslationGummyConnectionState::NotConfigured => "not_configured",
        TranslationGummyConnectionState::Connecting => "connecting",
        TranslationGummyConnectionState::Ready => "ready",
        TranslationGummyConnectionState::Reconnecting => "reconnecting",
        TranslationGummyConnectionState::Error => "error",
        TranslationGummyConnectionState::Stopped => "stopped",
    }
}

pub(super) fn status_label(
    text: &LocaleText,
    connection_state: TranslationGummyConnectionState,
) -> &'static str {
    match connection_state {
        TranslationGummyConnectionState::NotConfigured => {
            text.translation_gummy
                .translation_gummy_status_not_configured
        }
        TranslationGummyConnectionState::Connecting => {
            text.translation_gummy.translation_gummy_status_connecting
        }
        TranslationGummyConnectionState::Ready => {
            text.translation_gummy.translation_gummy_status_ready
        }
        TranslationGummyConnectionState::Reconnecting => {
            text.translation_gummy.translation_gummy_status_reconnecting
        }
        TranslationGummyConnectionState::Error => {
            text.translation_gummy.translation_gummy_status_error
        }
        TranslationGummyConnectionState::Stopped => {
            text.translation_gummy.translation_gummy_status_stopped
        }
    }
}

pub(super) fn reload_hotkeys() {
    crate::hotkey::reload_registrations();
}
