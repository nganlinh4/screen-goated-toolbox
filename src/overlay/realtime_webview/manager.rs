//! Parent-side lifecycle for the isolated realtime compositor.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::layout;
use super::protocol::{CardSettings, RealtimeScene};
use super::state::*;
use crate::APP;
use crate::api::realtime_audio::{RealtimeSessionPlan, start_realtime_transcription};

static PENDING_REALTIME_START: AtomicBool = AtomicBool::new(false);
static RELAY_STARTING: AtomicBool = AtomicBool::new(false);
static RELAY_READY: AtomicBool = AtomicBool::new(false);
static REGISTER_RELAY_CLASS: std::sync::Once = std::sync::Once::new();

fn session_transition_in_progress(pending_start: bool, stopping: bool) -> bool {
    pending_start || stopping
}

pub fn is_realtime_overlay_active() -> bool {
    if session_transition_in_progress(
        PENDING_REALTIME_START.load(Ordering::SeqCst),
        REALTIME_SESSION_STOPPING.load(Ordering::SeqCst),
    ) {
        return true;
    }
    unsafe { IS_ACTIVE }
}

pub fn stop_realtime_overlay() {
    PENDING_REALTIME_START.store(false, Ordering::SeqCst);
    super::controller::stop_runtime_flags();
    let hwnd = relay_hwnd();
    if hwnd.is_invalid() {
        finish_stop();
    } else {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_HIDE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn show_realtime_overlay() {
    let capability = crate::runtime_support::require_webview2("Realtime overlay");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    unsafe {
        if IS_ACTIVE || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst) {
            return;
        }
    }
    PENDING_REALTIME_START.store(true, Ordering::SeqCst);
    ensure_relay();
    post_pending_start();
}

pub(super) fn request_close_from_renderer() {
    let hwnd = relay_hwnd();
    if hwnd.is_invalid() {
        finish_stop();
        return;
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_HIDE, WPARAM(0), LPARAM(0));
    }
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    super::child::run()
}

fn ensure_relay() {
    if RELAY_READY.load(Ordering::SeqCst) || RELAY_STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("sgt-realtime-parent".to_string())
        .spawn(run_relay_loop)
        .expect("failed to start realtime parent relay");
}

fn run_relay_loop() {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(instance) => instance,
            Err(error) => {
                relay_failed(&format!("module handle unavailable: {error}"));
                return;
            }
        };
        let class_name = w!("SGTRealtimeParentRelay");
        REGISTER_RELAY_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(relay_window_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
        });
        let hwnd = match CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Realtime Relay"),
            WS_POPUP,
            -32_000,
            -32_000,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                relay_failed(&format!("window creation failed: {error}"));
                return;
            }
        };
        REALTIME_HWND = hwnd;
        RELAY_READY.store(true, Ordering::SeqCst);
        RELAY_STARTING.store(false, Ordering::SeqCst);
        post_pending_start();

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        RELAY_READY.store(false, Ordering::SeqCst);
        RELAY_STARTING.store(false, Ordering::SeqCst);
        REALTIME_HWND = HWND::default();
    }
}

unsafe extern "system" fn relay_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_APP_REALTIME_START {
            handle_start_overlay();
            return LRESULT(0);
        }
        super::wndproc::realtime_wnd_proc(hwnd, message, wparam, lparam)
    }
}

fn post_pending_start() {
    if !RELAY_READY.load(Ordering::SeqCst) {
        return;
    }
    if !PENDING_REALTIME_START.load(Ordering::SeqCst) {
        return;
    }
    let hwnd = relay_hwnd();
    if !hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_START, WPARAM(0), LPARAM(0));
        }
    }
}

fn relay_failed(reason: &str) {
    RELAY_STARTING.store(false, Ordering::SeqCst);
    PENDING_REALTIME_START.store(false, Ordering::SeqCst);
    crate::log_info!("[RealtimeCompositor] parent relay failed: {reason}");
}

fn relay_hwnd() -> HWND {
    unsafe { std::ptr::addr_of!(REALTIME_HWND).read() }
}

fn handle_start_overlay() {
    if PENDING_REALTIME_START
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    unsafe {
        if IS_ACTIVE || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst) {
            return;
        }
    }
    let session_config = super::controller::load_session_config();
    let (translation_size, transcription_size) = {
        let app = APP.lock().unwrap();
        (
            app.config.realtime_translation_size,
            app.config.realtime_transcription_size,
        )
    };
    super::controller::reset_runtime_for_new_session();
    let target_language = resolve_target_language(&session_config.target_language);
    let mut active_config = session_config;
    active_config.target_language.clone_from(&target_language);
    super::controller::apply_session_config(&active_config);
    let has_translation = true;
    let layout = initial_layout(transcription_size, translation_size, has_translation);
    MIC_VISIBLE.store(true, Ordering::SeqCst);
    TRANS_VISIBLE.store(has_translation, Ordering::SeqCst);
    let settings = CardSettings {
        audio_source: active_config.audio_source.clone(),
        target_language,
        translation_model: active_config.translation_model.clone(),
        transcription_model: active_config.transcription_model.clone(),
        transcription_language: active_config.transcription_language.to_uppercase(),
        font_size: active_config.font_size,
    };
    super::parent::replace_scene(RealtimeScene {
        active: true,
        layout,
        settings,
        tts_speed: 100,
        translation_model: active_config.translation_model,
        is_dark: crate::overlay::is_dark_mode(),
        ..Default::default()
    });

    let hwnd = relay_hwnd();
    let translation_hwnd = has_translation.then_some(hwnd);
    start_realtime_transcription(
        RealtimeSessionPlan {
            audio_source: active_config.audio_source,
            target_language: active_config.target_language,
            has_translation,
        },
        current_stop_signal(),
        hwnd,
        translation_hwnd,
        REALTIME_STATE.clone(),
    );
}

fn resolve_target_language(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    "English".to_string()
}

fn initial_layout(
    transcription_size: (i32, i32),
    translation_size: (i32, i32),
    has_translation: bool,
) -> layout::CompositorLayout {
    unsafe {
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let total_width = if has_translation {
            transcription_size.0 + translation_size.0 + GAP
        } else {
            transcription_size.0
        };
        let x = (screen_width - total_width) / 2;
        let y = (screen_height - transcription_size.1) / 2;
        layout::CompositorLayout {
            transcription: layout::CardRect {
                x,
                y,
                width: transcription_size.0,
                height: transcription_size.1,
                visible: true,
            },
            translation: layout::CardRect {
                x: x + transcription_size.0 + GAP,
                y,
                width: translation_size.0,
                height: translation_size.1,
                visible: has_translation,
            },
        }
    }
}

pub(super) fn finish_stop() {
    unsafe {
        IS_ACTIVE = false;
    }
    REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
    super::parent::set_active(false);
}

#[cfg(test)]
mod tests {
    use super::{initial_layout, session_transition_in_progress};

    #[test]
    fn pending_or_stopping_session_remains_available_to_the_toggle() {
        assert!(session_transition_in_progress(true, false));
        assert!(session_transition_in_progress(false, true));
        assert!(!session_transition_in_progress(false, false));
    }

    #[test]
    fn initial_layout_keeps_both_cards_in_one_compositor_coordinate_space() {
        let layout = initial_layout((400, 300), (500, 250), true);
        assert_eq!(layout.translation.x - layout.transcription.x, 420);
        assert!(layout.transcription.visible);
        assert!(layout.translation.visible);
    }
}
