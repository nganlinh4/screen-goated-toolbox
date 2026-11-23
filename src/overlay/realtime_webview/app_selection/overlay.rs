use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::api::realtime_audio::{WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};
use crate::gui::locale::LocaleText;
use crate::overlay::window_selector::{
    self, SelectorCallbacks, SelectorEntry, SelectorOwner, SelectorText,
};

use super::data::{AudioAppCandidate, enumerate_audio_app_candidates, refresh_audio_capture_pid};
use super::{clear_selected_audio_app_candidate, store_selected_audio_app_candidate};
use crate::overlay::realtime_webview::state::{
    CLOSE_TTS_MODAL_REQUEST, CURRENT_TTS_SPEED, NEW_AUDIO_SOURCE, REALTIME_HWND,
    REALTIME_SESSION_ID, REALTIME_SESSION_PUBLICATION, REALTIME_SESSION_STOPPING,
    REALTIME_TTS_ENABLED, REALTIME_TTS_SPEED, SELECTED_APP_NAME, SELECTED_APP_PID,
    TRANSCRIPTION_ENGINE_REVISION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Owner {
    id: u64,
    session: u64,
    engine: u64,
    renderer: u64,
}
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
static ACTIVE_OWNER: LazyLock<Mutex<Option<Owner>>> = LazyLock::new(|| Mutex::new(None));

fn owner_current(owner: Owner) -> bool {
    let active = *ACTIVE_OWNER.lock().unwrap_or_else(|e| e.into_inner());
    owner_matches(
        active,
        owner,
        REALTIME_SESSION_ID.load(Ordering::SeqCst),
        TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst),
        REALTIME_SESSION_STOPPING.load(Ordering::SeqCst)
            || crate::overlay::realtime_webview::state::current_stop_signal()
                .load(Ordering::SeqCst),
        owner.renderer == 0
            || crate::overlay::realtime_webview::supervisor::is_current_generation(owner.renderer),
    )
}

fn owner_matches(
    active: Option<Owner>,
    owner: Owner,
    session: u64,
    engine: u64,
    stopped: bool,
    renderer_current: bool,
) -> bool {
    active == Some(owner)
        && session == owner.session
        && engine == owner.engine
        && !stopped
        && renderer_current
}

fn finish_owner(owner: Owner) {
    let _publication = REALTIME_SESSION_PUBLICATION
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut active = ACTIVE_OWNER.lock().unwrap_or_else(|e| e.into_inner());
    if *active == Some(owner) {
        *active = None;
    }
}

pub(in crate::overlay::realtime_webview) fn owner_id_for_session(session: u64) -> Option<u64> {
    ACTIVE_OWNER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .filter(|owner| owner.session == session)
        .map(|owner| owner.id)
}

/// Caller holds REALTIME_SESSION_PUBLICATION; close the returned token after unlocking.
pub(in crate::overlay::realtime_webview) fn retire_owner_for_session_locked(
    session: u64,
) -> Option<u64> {
    let mut active = ACTIVE_OWNER.lock().unwrap_or_else(|e| e.into_inner());
    if active.is_some_and(|owner| owner.session == session) {
        active.take().map(|owner| owner.id)
    } else {
        None
    }
}

struct SelectorLease(Owner);
impl Drop for SelectorLease {
    fn drop(&mut self) {
        finish_owner(self.0);
    }
}

fn post_realtime_updates() {
    unsafe {
        let realtime_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if !realtime_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(realtime_hwnd),
                WM_TRANSLATION_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
            let _ = PostMessageW(
                Some(realtime_hwnd),
                WM_REALTIME_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn apply_audio_app_selection(
    owner: Owner,
    pid: u32,
    name: &str,
    candidate: Option<AudioAppCandidate>,
) {
    crate::log_info!(
        "[AppSelection] selected audio capture pid={} name={}",
        pid,
        name
    );
    let committed = crate::overlay::realtime_webview::controller::publish_audio_selection(
        owner.session,
        owner.engine,
        || owner_current(owner),
        || {
            if let Some(candidate) = candidate {
                store_selected_audio_app_candidate(candidate);
            }
            SELECTED_APP_PID.store(pid, Ordering::SeqCst);
            if let Ok(mut app_name) = SELECTED_APP_NAME.lock() {
                *app_name = name.to_string();
            }
            crate::overlay::realtime_webview::controller::reset_read_cursor();
            if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
                *new_source = "device".to_string();
            }
            REALTIME_TTS_ENABLED.store(true, Ordering::SeqCst);
            CLOSE_TTS_MODAL_REQUEST.store(true, Ordering::SeqCst);
            let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
            CURRENT_TTS_SPEED.store(base_speed, Ordering::Relaxed);
        },
    );
    finish_owner(owner);
    if committed {
        post_realtime_updates();
    }
}

fn cancel_audio_app_selection(owner: Owner) {
    let is_s2s = crate::model_config::is_gemini_live_s2s_model_id(
        &crate::overlay::realtime_webview::controller::load_session_config().transcription_model,
    );
    let committed = crate::overlay::realtime_webview::controller::publish_audio_selection(
        owner.session,
        owner.engine,
        || owner_current(owner),
        || {
            clear_selected_audio_app_candidate();
            REALTIME_TTS_ENABLED.store(is_s2s, Ordering::SeqCst);
            crate::overlay::realtime_webview::controller::reset_read_cursor();

            SELECTED_APP_PID.store(0, Ordering::SeqCst);
            if let Ok(mut app_name) = SELECTED_APP_NAME.lock() {
                app_name.clear();
            }
            if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
                *new_source = "device".to_string();
            }

            let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
            CURRENT_TTS_SPEED.store(base_speed, Ordering::Relaxed);
        },
    );
    finish_owner(owner);
    if committed {
        post_realtime_updates();
    }
}

fn spawn_thumbnail_loader(owner: Owner, candidates: Vec<AudioAppCandidate>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(280));

        for candidate in candidates {
            if !owner_current(owner)
                || !window_selector::is_owner_active(SelectorOwner::RealtimeAppSelection)
            {
                break;
            }
            if candidate.window_hwnd == 0 {
                continue;
            }

            let hwnd = HWND(candidate.window_hwnd as *mut std::ffi::c_void);
            if hwnd.is_invalid() {
                continue;
            }

            if let Some(data_url) = crate::overlay::screen_record::capture_window_thumbnail(hwnd)
                && owner_current(owner)
            {
                window_selector::post_preview_update_for_token(
                    SelectorOwner::RealtimeAppSelection,
                    owner.id,
                    &candidate.pid.to_string(),
                    data_url,
                );
            }
        }
    });
}

pub(crate) fn show_audio_app_selector_for_session(
    session: u64,
    engine: u64,
    stop: Arc<AtomicBool>,
    expected_renderer: Option<u64>,
) {
    if expected_renderer == Some(0) {
        return;
    }
    let renderer = expected_renderer.unwrap_or(0);
    let Ok(id) = NEXT_OWNER.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
        v.checked_add(1)
            .filter(|next| usize::try_from(*next).is_ok())
    }) else {
        return;
    };
    let owner = Owner {
        id,
        session,
        engine,
        renderer,
    };
    let mut installed = false;
    let mut replaced = None;
    crate::overlay::realtime_webview::state::publish_if_engine_current(
        session,
        &stop,
        engine,
        || {
            if renderer != 0
                && !crate::overlay::realtime_webview::supervisor::is_current_generation(renderer)
            {
                return;
            }
            let mut active = ACTIVE_OWNER.lock().unwrap_or_else(|e| e.into_inner());
            if active.is_some_and(|old| {
                old.session == session && old.engine == engine && old.renderer == renderer
            }) {
                return;
            }
            replaced = active.replace(owner).map(|old| old.id);
            installed = true;
        },
    );
    if !installed {
        return;
    }
    if let Some(token) = replaced {
        window_selector::close_selector_for_token(SelectorOwner::RealtimeAppSelection, token);
    }
    if !owner_current(owner) {
        finish_owner(owner);
        return;
    }

    let candidates = enumerate_audio_app_candidates();
    if !owner_current(owner) {
        finish_owner(owner);
        return;
    }
    if candidates.is_empty() {
        eprintln!("[AppSelection] No visible audio apps found");
        cancel_audio_app_selection(owner);
        return;
    }

    let (language, is_dark) = {
        let app = crate::APP.lock().unwrap();
        let is_dark = app.config.theme_mode.is_dark();
        (app.config.ui_language.clone(), is_dark)
    };
    let locale = LocaleText::get(&language);
    let app_names: HashMap<String, String> = candidates
        .iter()
        .map(|candidate| (candidate.pid.to_string(), candidate.display_name.clone()))
        .collect();
    let app_candidates: HashMap<String, AudioAppCandidate> = candidates
        .iter()
        .map(|candidate| (candidate.pid.to_string(), candidate.clone()))
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
            selection_notice: None,
            disabled: false,
        })
        .collect();
    let callbacks = SelectorCallbacks::new(
        move |selected_id| {
            if !owner_current(owner) {
                finish_owner(owner);
                return;
            }
            if let Ok(pid) = selected_id.parse::<u32>() {
                let name = app_names
                    .get(&selected_id)
                    .cloned()
                    .unwrap_or_else(|| format!("PID {pid}"));
                let candidate = app_candidates.get(&selected_id).cloned();
                let capture_pid = candidate
                    .as_ref()
                    .map(refresh_audio_capture_pid)
                    .unwrap_or(pid);
                apply_audio_app_selection(owner, capture_pid, &name, candidate);
            }
        },
        move || cancel_audio_app_selection(owner),
    );

    let lease = Arc::new(SelectorLease(owner));
    let valid_lease = lease.clone();
    let admission_lease = lease;
    window_selector::show_selector_guarded(
        SelectorOwner::RealtimeAppSelection,
        entries,
        is_dark,
        SelectorText {
            title: locale.realtime.app_select_title.to_string(),
            subtitle: locale.realtime.app_select_hint.to_string(),
            count_label: locale
                .realtime
                .app_select_count
                .replace("{}", &candidates.len().to_string()),
            cancel_label: locale.preset_basics.cancel_label.to_string(),
        },
        callbacks,
        window_selector::SelectorGuard {
            valid: Some(Arc::new(move || {
                let _ = &valid_lease;
                owner_current(owner)
            })),
            admission: Some(Arc::new(move |effect| {
                let _ = &admission_lease;
                let mut admitted = false;
                crate::overlay::realtime_webview::state::publish_if_engine_current(
                    owner.session,
                    &stop,
                    owner.engine,
                    || {
                        if owner_current(owner) {
                            effect();
                            admitted = true;
                        }
                    },
                );
                admitted
            })),
            token: owner.id,
        },
    );

    spawn_thumbnail_loader(owner, candidates);
}

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod ownership_tests;
