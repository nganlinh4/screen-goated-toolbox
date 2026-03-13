use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::api::realtime_audio::{WM_EXEC_SCRIPT, WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};
use crate::gui::locale::LocaleText;
use crate::overlay::window_selector::{
    self, SelectorCallbacks, SelectorEntry, SelectorOwner, SelectorText,
};

use super::data::{AudioAppCandidate, enumerate_audio_app_candidates};
use crate::overlay::realtime_webview::state::{
    AUDIO_SOURCE_CHANGE, CLOSE_TTS_MODAL_REQUEST, COMMITTED_TRANSLATION_QUEUE, CURRENT_TTS_SPEED,
    LAST_SPOKEN_LENGTH, NEW_AUDIO_SOURCE, REALTIME_HWND, REALTIME_TTS_ENABLED, REALTIME_TTS_SPEED,
    SELECTED_APP_NAME, SELECTED_APP_PID, TRANSLATION_HWND,
};

fn post_realtime_updates() {
    unsafe {
        let translation_hwnd = std::ptr::addr_of!(TRANSLATION_HWND).read();
        if !translation_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(translation_hwnd),
                WM_TRANSLATION_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
        }

        let realtime_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if !realtime_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(realtime_hwnd),
                WM_REALTIME_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn push_script_to_realtime_windows(script: String) {
    let windows = unsafe {
        [
            std::ptr::addr_of!(REALTIME_HWND).read(),
            std::ptr::addr_of!(TRANSLATION_HWND).read(),
        ]
    };

    for hwnd in windows {
        if hwnd.is_invalid() {
            continue;
        }

        let script_ptr = Box::into_raw(Box::new(script.clone()));
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_EXEC_SCRIPT,
                WPARAM(0),
                LPARAM(script_ptr as isize),
            );
        }
    }
}

fn apply_audio_app_selection(pid: u32, name: &str) {
    SELECTED_APP_PID.store(pid, Ordering::SeqCst);
    if let Ok(mut app_name) = SELECTED_APP_NAME.lock() {
        *app_name = name.to_string();
    }
    if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
        *new_source = "device".to_string();
    }
    AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
    CLOSE_TTS_MODAL_REQUEST.store(true, Ordering::SeqCst);
    post_realtime_updates();
}

fn cancel_audio_app_selection() {
    REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    crate::api::tts::TTS_MANAGER.stop();

    LAST_SPOKEN_LENGTH.store(0, Ordering::SeqCst);
    if let Ok(mut queue) = COMMITTED_TRANSLATION_QUEUE.lock() {
        queue.clear();
    }

    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut app_name) = SELECTED_APP_NAME.lock() {
        app_name.clear();
    }

    let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
    CURRENT_TTS_SPEED.store(base_speed, Ordering::Relaxed);
    push_script_to_realtime_windows(format!(
        "if(window.setTtsEnabled) window.setTtsEnabled(false); if(window.updateTtsSpeed) window.updateTtsSpeed({base_speed});"
    ));
    post_realtime_updates();
}

fn spawn_thumbnail_loader(candidates: Vec<AudioAppCandidate>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(280));

        for candidate in candidates {
            if !window_selector::is_owner_active(SelectorOwner::RealtimeAppSelection) {
                break;
            }

            let hwnd = HWND(candidate.window_hwnd as *mut std::ffi::c_void);
            if hwnd.is_invalid() {
                continue;
            }

            if let Some(data_url) = crate::overlay::screen_record::capture_window_thumbnail(hwnd) {
                window_selector::post_preview_update_for_owner(
                    SelectorOwner::RealtimeAppSelection,
                    &candidate.pid.to_string(),
                    data_url,
                );
            }
        }
    });
}

pub fn show_audio_app_selector_overlay() {
    let candidates = enumerate_audio_app_candidates();
    if candidates.is_empty() {
        eprintln!("[AppSelection] No visible audio apps found");
        cancel_audio_app_selection();
        return;
    }

    let (language, is_dark) = {
        let app = crate::APP.lock().unwrap();
        let is_dark = match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };
        (app.config.ui_language.clone(), is_dark)
    };
    let locale = LocaleText::get(&language);
    let app_names: HashMap<String, String> = candidates
        .iter()
        .map(|candidate| (candidate.pid.to_string(), candidate.display_name.clone()))
        .collect();
    let entries: Vec<SelectorEntry> = candidates
        .iter()
        .map(|candidate| SelectorEntry {
            id: candidate.pid.to_string(),
            title: candidate.display_name.clone(),
            subtitle: candidate.process_name.clone(),
            icon_data_url: candidate.icon_data_url.clone(),
            preview_data_url: None,
            width: candidate.width,
            height: candidate.height,
            badge_text: None,
            disabled: false,
        })
        .collect();
    let callbacks = SelectorCallbacks::new(
        move |selected_id| {
            if let Ok(pid) = selected_id.parse::<u32>() {
                let name = app_names
                    .get(&selected_id)
                    .cloned()
                    .unwrap_or_else(|| format!("PID {pid}"));
                apply_audio_app_selection(pid, &name);
            }
        },
        cancel_audio_app_selection,
    );

    window_selector::show_selector(
        SelectorOwner::RealtimeAppSelection,
        entries,
        is_dark,
        SelectorText {
            title: locale.app_select_title.to_string(),
            subtitle: locale.app_select_hint.to_string(),
            count_label: locale
                .app_select_count
                .replace("{}", &candidates.len().to_string()),
            cancel_label: locale.cancel_label.to_string(),
        },
        callbacks,
    );

    spawn_thumbnail_loader(candidates);
}
