use std::collections::HashMap;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::overlay::realtime_webview::app_selection::{
    AudioAppCandidate, enumerate_audio_app_candidates,
};
use crate::overlay::window_selector::{
    self, SelectorCallbacks, SelectorEntry, SelectorOwner, SelectorText,
};

use super::{SR_HWND, WM_APP_RUN_SCRIPT};

fn post_screen_record_script(script: String) {
    let sr_hwnd_val = unsafe { std::ptr::addr_of!(SR_HWND).read().0.0 as isize };
    if sr_hwnd_val == 0 {
        return;
    }

    let script_ptr = Box::into_raw(Box::new(script));
    unsafe {
        let _ = PostMessageW(
            Some(HWND(sr_hwnd_val as *mut _)),
            WM_APP_RUN_SCRIPT,
            WPARAM(0),
            LPARAM(script_ptr as isize),
        );
    }
}

fn spawn_thumbnail_loader(candidates: Vec<AudioAppCandidate>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(280));

        for candidate in candidates {
            if !window_selector::is_owner_active(SelectorOwner::ScreenRecordAudioAppSelection) {
                break;
            }

            let hwnd = HWND(candidate.window_hwnd as *mut std::ffi::c_void);
            if hwnd.is_invalid() {
                continue;
            }

            if let Some(data_url) = crate::overlay::screen_record::capture_window_thumbnail(hwnd) {
                window_selector::post_preview_update_for_owner(
                    SelectorOwner::ScreenRecordAudioAppSelection,
                    &candidate.pid.to_string(),
                    data_url,
                );
            }
        }
    });
}

pub fn show_audio_app_selector(is_dark: bool, lang: String) {
    let candidates = enumerate_audio_app_candidates();
    if candidates.is_empty() {
        post_screen_record_script(
            "window.dispatchEvent(new CustomEvent('external-recording-audio-app-selection-cancelled'))"
                .to_string(),
        );
        return;
    }

    let locale = crate::gui::locale::LocaleText::get(&lang);
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
                let app_name = app_names
                    .get(&selected_id)
                    .cloned()
                    .unwrap_or_else(|| format!("PID {pid}"));
                post_screen_record_script(format!(
                    "window.dispatchEvent(new CustomEvent('external-recording-audio-app-selected',{{detail:{{pid:{pid},appName:{}}}}}))",
                    serde_json::to_string(&app_name).unwrap_or_else(|_| "\"\"".to_string())
                ));
            }
        },
        || {
            post_screen_record_script(
                "window.dispatchEvent(new CustomEvent('external-recording-audio-app-selection-cancelled'))"
                    .to_string(),
            );
        },
    );

    window_selector::show_selector(
        SelectorOwner::ScreenRecordAudioAppSelection,
        entries,
        is_dark,
        SelectorText {
            title: locale.app_select_title.to_string(),
            subtitle: String::new(),
            count_label: locale
                .app_select_count
                .replace("{}", &candidates.len().to_string()),
            cancel_label: locale.cancel_label.to_string(),
        },
        callbacks,
    );

    spawn_thumbnail_loader(candidates);
}
