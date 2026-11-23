// --- TEXT INPUT MODULE ---
// Text input overlay for user prompts with WebView-based editor.

mod activation;
mod delivery;
mod editor_text;
mod focus;
mod key_ownership;
mod messages;
mod passive;
mod state;
mod styles;
mod window;

use crate::gui::locale::LocaleText;
use state::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationMode {
    Focused,
    Passive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShowOptions {
    pub activation_mode: ActivationMode,
}

impl Default for ShowOptions {
    fn default() -> Self {
        Self {
            activation_mode: ActivationMode::Focused,
        }
    }
}

// --- PUBLIC API ---

pub fn is_active() -> bool {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return false;
    }
    unsafe {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        // Since we use offscreen WS_VISIBLE, we must check coordinates
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            return rect.left > -3000;
        }
        false
    }
}

pub fn cancel_input() {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn update_theme(is_dark: bool) {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_UPDATE_THEME,
                WPARAM(usize::from(is_dark)),
                LPARAM(0),
            );
        }
    }
}

pub(crate) fn try_active_session() -> anyhow::Result<Option<u64>> {
    EDITOR_SESSIONS.try_current()
}

pub(crate) fn set_editor_text_for_session(
    text: &str,
    owner: Arc<crate::overlay::result::ChainCancelToken>,
    session: u64,
) {
    if text.len() > MAX_PENDING_TEXT_BYTES {
        if EDITOR_SESSIONS.is_current(session) && !owner.is_cancelled() {
            notify_text_delivery(false);
        }
        return;
    }
    queue_editor_action(session, text.to_string(), EditorAction::Insert, Some(owner));
}

fn queue_editor_text(session: u64, text: String) {
    queue_editor_action(session, text, EditorAction::Replace, None);
}

fn queue_editor_action(
    session: u64,
    text: String,
    action: EditorAction,
    owner: Option<Arc<crate::overlay::result::ChainCancelToken>>,
) {
    let user_visible = !matches!(&action, EditorAction::Focus { .. });
    let Some(operation) = create_text_operation(session, owner.as_ref(), user_visible) else {
        if user_visible
            && EDITOR_SESSIONS.is_current(session)
            && owner.as_ref().is_none_or(|token| !token.is_cancelled())
        {
            notify_text_delivery(false);
        }
        return;
    };
    let pending = PendingText {
        text,
        action,
        session,
        owner: owner.as_ref().map(|token| EditorOwner {
            token: token.clone(),
            session,
        }),
        operation: Some(operation.clone()),
    };
    let admitted = if let Some(owner) = owner {
        owner.run_if_active(|| EDITOR_SESSIONS.enqueue(pending))
    } else {
        Some(EDITOR_SESSIONS.enqueue(pending))
    };
    if admitted == Some(true) {
        if !wake_editor_queue() {
            EDITOR_SESSIONS.remove_pending(operation.id());
            operation.fail_uncertain();
        }
    } else {
        operation.cancel();
        if user_visible && admitted == Some(false) && EDITOR_SESSIONS.is_current(session) {
            notify_text_delivery(false);
        }
    }
}

fn create_text_operation(
    session: u64,
    owner: Option<&Arc<crate::overlay::result::ChainCancelToken>>,
    user_visible: bool,
) -> Option<Arc<delivery::Operation>> {
    let nonce = DOCUMENT_NONCE.lock().unwrap().clone()?;
    let revoke: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(|id| {
        let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
        let posted = hwnd_val != 0
            && unsafe {
                PostMessageW(
                    Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                    WM_APP_REVOKE_TEXT,
                    WPARAM(id as usize),
                    LPARAM(0),
                )
                .is_ok()
            };
        if !posted && let Some(operation) = delivery::get(id) {
            operation.fail_uncertain();
        }
    });
    let operation = delivery::Operation::new(session, nonce, owner, revoke)?;
    let operation_id = operation.id();
    operation.observe(Arc::new(move |outcome| {
        if matches!(
            outcome,
            delivery::Settlement::Uncertain
                | delivery::Settlement::AppliedBeforeCancellationSettled
        ) {
            crate::log_info!("[TextInput] Renderer operation delivery is uncertain");
            if user_visible {
                notify_text_delivery(true);
            }
        }
        if user_visible
            && outcome == delivery::Settlement::Rejected
            && EDITOR_SESSIONS.is_current(session)
        {
            notify_text_delivery(false);
        }
    }));
    operation.observe_quiescence(Arc::new(move || {
        if EDITOR_SESSIONS.finish_operation(operation_id)
            && EDITOR_SESSIONS.current() == Some(EDITOR_READY.load(Ordering::SeqCst))
        {
            wake_editor_queue();
        }
    }));
    Some(operation)
}

fn notify_text_delivery(uncertain: bool) {
    let language = CFG_LANG.lock().unwrap().clone();
    let text = crate::gui::locale::LocaleText::get(&language).shell;
    crate::overlay::auto_copy_badge::show_error_notification(if uncertain {
        text.text_input_delivery_uncertain
    } else {
        text.text_input_delivery_failed
    });
}

fn wake_editor_queue() -> bool {
    // Post message to the text input window to trigger the injection
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            return PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_SET_TEXT,
                WPARAM(0),
                LPARAM(0),
            )
            .is_ok();
        }
    }
    false
}

/// Update the UI text (header) and trigger a repaint
pub fn update_ui_text_for_session(session: u64, header_text: String) {
    if !is_callback_owner(session) {
        return;
    }
    *CFG_TITLE.lock().unwrap() = header_text.clone();
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            let _ = SetWindowTextW(hwnd, &windows::core::HSTRING::from(header_text));
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(1), LPARAM(session as isize));
        }
    }
}

pub fn is_callback_owner(session: u64) -> bool {
    session != 0 && CALLBACK_OWNER.load(Ordering::SeqCst) == session
}

pub fn close_for_session(session: u64) {
    if !is_callback_owner(session) {
        return;
    }
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_CLOSE_SESSION,
                WPARAM(session as usize),
                LPARAM(0),
            );
        }
    }
}

pub(crate) fn request_focus_for_session(
    hwnd: crate::win_types::SendHwnd,
    session: u64,
    owner: Arc<crate::overlay::result::ChainCancelToken>,
) {
    if INPUT_HWND.load(Ordering::SeqCst) == hwnd.as_isize() {
        queue_editor_action(
            session,
            String::new(),
            EditorAction::Focus { retry: false },
            Some(owner),
        );
    }
}

/// Bring the text input window to foreground and focus the editor
/// Call this after closing modal windows like the preset wheel
pub fn refocus_editor_for_session(session: u64) {
    queue_editor_action(
        session,
        String::new(),
        EditorAction::Focus { retry: false },
        None,
    );
}

fn refocus_editor_guarded(owner: Option<EditorOwner>, session: u64) {
    let allowed = || {
        EDITOR_SESSIONS.is_current(session)
            && owner
                .as_ref()
                .is_none_or(|owner| !owner.token.is_cancelled())
    };
    if !allowed() {
        return;
    }
    if PASSIVE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        return;
    }

    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
            use windows::Win32::UI::WindowsAndMessaging::{BringWindowToTop, SetForegroundWindow};

            // Aggressive focus: try multiple methods
            let _ = BringWindowToTop(hwnd);
            if !allowed() {
                return;
            }
            let _ = SetForegroundWindow(hwnd);
            if !allowed() {
                return;
            }
            let _ = SetFocus(Some(hwnd));
            if !allowed() {
                return;
            }

            // Focus the webview editor immediately
            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    // First focus the WebView itself (native focus)
                    let _ = wv.focus();
                }
            });
        }
    }
}

/// Get the current window rect of the text input window (if active)
pub fn get_window_rect() -> Option<RECT> {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(HWND(hwnd_val as *mut std::ffi::c_void), &mut rect).is_ok() {
                return Some(rect);
            }
        }
    }
    None
}

/// Start the persistent hidden window (called from main)
pub fn warmup() {
    // Thread-safe atomic check-and-set to prevent multiple warmup threads
    if IS_WARMED_UP.load(Ordering::SeqCst) {
        return;
    }
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        window::internal_create_window_loop();
    });
}

pub fn show(
    prompt_guide: String,
    ui_language: String,
    cancel_hotkey_name: String,
    continuous_mode: bool,
    on_submit: impl Fn(String, HWND, u64) + Send + Sync + 'static,
) {
    show_with_options(
        prompt_guide,
        ui_language,
        cancel_hotkey_name,
        continuous_mode,
        ShowOptions::default(),
        on_submit,
    );
}

pub fn show_with_options(
    prompt_guide: String,
    ui_language: String,
    cancel_hotkey_name: String,
    continuous_mode: bool,
    options: ShowOptions,
    on_submit: impl Fn(String, HWND, u64) + Send + Sync + 'static,
) {
    let capability =
        crate::runtime_support::require_webview2(crate::runtime_support::WebViewFeature::TextInput);
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    // Re-entrancy guard: if we are already in the process of showing/waiting, ignore subsequent calls
    // This prevents key-mashing from spawning multiple wait loops or confused states
    if IS_SHOWING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    // Ensure we clear the flag when we return
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            IS_SHOWING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    // Clone lang for locale notification before moving/consuming it
    let lang_for_locale = ui_language.clone();

    // The active callback and editor configuration are changed together by
    // the window thread, after any earlier input has been dispatched.
    *PENDING_SHOW.lock().unwrap() = Some(PendingShow {
        title: prompt_guide,
        language: ui_language,
        cancel_hotkey: cancel_hotkey_name,
        continuous: continuous_mode,
        passive: matches!(options.activation_mode, ActivationMode::Passive),
        callback: Arc::new(on_submit),
    });

    // Check if warmed up
    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        // Trigger warmup for recovery
        warmup();

        // Show localized message that feature is not ready yet
        let locale = LocaleText::get(&lang_for_locale);
        crate::overlay::auto_copy_badge::show_notification(locale.shell.text_input_loading);

        // Blocking wait with message pump
        // We wait up to 20 seconds. If it fails, we simply return (preventing premature broken window)
        for _ in 0..2000 {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(10));

            if IS_WARMED_UP.load(Ordering::SeqCst) {
                break;
            }
        }

        // If still not warmed up after wait, give up
        if !IS_WARMED_UP.load(Ordering::SeqCst) {
            return;
        }
    }

    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            // ALWAYS show logic (Toggle logic handled by caller if needed)
            // Fixes issue where dynamic prompt mode fails to appear if window state is desync
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}
