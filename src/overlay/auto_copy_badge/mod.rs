use crate::APP;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex, Once};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{WebContext, WebView};

#[path = "../auto_copy_badge_html.rs"]
mod html;
mod messages;
mod window;

pub(super) static REGISTER_BADGE_CLASS: Once = Once::new();

// Thread-safe handle using atomic (like preset_wheel)
pub(super) static BADGE_HWND: AtomicIsize = AtomicIsize::new(0);
pub(super) static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
pub(super) static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);

// Messages
pub(super) const WM_APP_PROCESS_QUEUE: u32 = WM_USER + 201;
pub(super) const WM_APP_UPDATE_PROGRESS: u32 = WM_USER + 202;
pub(super) const WM_APP_HIDE_PROGRESS: u32 = WM_USER + 203;
pub(super) const WM_APP_HIDE_BADGE: u32 = WM_USER + 204;

/// Notification themes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotificationType {
    Success,  // Green - auto copied
    FileCopy, // Cyan - copied media file
    GifCopy,  // Pink - copied GIF file
    Info,     // Yellow - loading/warming up
    Update,   // Blue - update available (longer duration)
    Error,    // Red - error (e.g., no writable area for auto-paste)
}

#[derive(Clone, Debug)]
pub(super) struct PendingNotification {
    pub title: String,
    pub snippet: String,
    pub n_type: NotificationType,
    pub duration_ms: Option<u32>,
}

#[derive(Clone, Debug)]
pub(super) struct ProgressNotification {
    pub title: String,
    pub snippet: String,
    pub progress: f32,
}

pub(super) static PENDING_QUEUE: LazyLock<Mutex<VecDeque<PendingNotification>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
pub(super) static ACTIVE_PROGRESS: LazyLock<Mutex<Option<ProgressNotification>>> =
    LazyLock::new(|| Mutex::new(None));

thread_local! {
    pub(super) static BADGE_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    pub(super) static BADGE_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

// Dimensions
pub(super) const BADGE_WIDTH: i32 = 1200; // Super wide
pub(super) const BADGE_HEIGHT: i32 = 400; // Taller for stacking

pub fn enqueue_notification_with_duration(
    title: String,
    snippet: String,
    n_type: NotificationType,
    duration_ms: Option<u32>,
) {
    crate::log_info!(
        "[Badge] Enqueuing: '{}' ({:?}, duration_ms={:?})",
        title,
        n_type,
        duration_ms
    );
    {
        let mut q = PENDING_QUEUE.lock().unwrap();
        q.push_back(PendingNotification {
            title,
            snippet,
            n_type,
            duration_ms,
        });
    }
    ensure_window_and_post(WM_APP_PROCESS_QUEUE);
}

fn enqueue_notification(title: String, snippet: String, n_type: NotificationType) {
    enqueue_notification_with_duration(title, snippet, n_type, None);
}

pub fn show_auto_copy_badge_text(text: &str) {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.shell.auto_copied_badge.to_string();
    drop(app);

    let clean_text = text.replace('\n', " ").replace('\r', "");
    let snippet = format!("\"{}\"", clean_text);

    enqueue_notification(title, snippet, NotificationType::Success);
}

pub fn show_auto_copy_badge_image() {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.shell.auto_copied_badge.to_string();
    let snippet = locale.shell.auto_copied_image_badge.to_string();
    drop(app);

    enqueue_notification(title, snippet, NotificationType::Success);
}

pub fn show_auto_copy_badge_media_file(file_path: &str) {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.shell.auto_copied_badge.to_string();
    drop(app);

    let display_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file_path)
        .replace('\n', " ")
        .replace('\r', "");
    let is_gif = std::path::Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gif"))
        .unwrap_or(false);

    let snippet = format!("\"{}\"", display_name);
    // Media file copy confirmation uses dedicated themes/icons so it stands out from normal text copy.
    let media_type = if is_gif {
        NotificationType::GifCopy
    } else {
        NotificationType::FileCopy
    };
    enqueue_notification_with_duration(title, snippet, media_type, Some(2400));
}

/// Show a loading/info notification with just a title (yellow theme)
pub fn show_notification(title: &str) {
    enqueue_notification(title.to_string(), String::new(), NotificationType::Info);
}

/// Show an update available notification (blue theme, longer duration)
pub fn show_update_notification(title: &str) {
    enqueue_notification(title.to_string(), String::new(), NotificationType::Update);
}

/// Show an error notification (red theme)
pub fn show_error_notification(title: &str) {
    enqueue_notification(title.to_string(), String::new(), NotificationType::Error);
}

/// Show a detailed notification with title and snippet (custom type)
pub fn show_detailed_notification(title: &str, snippet: &str, n_type: NotificationType) {
    enqueue_notification(title.to_string(), snippet.to_string(), n_type);
}

pub fn show_timed_detailed_notification(
    title: &str,
    snippet: &str,
    n_type: NotificationType,
    duration_ms: u32,
) {
    enqueue_notification_with_duration(
        title.to_string(),
        snippet.to_string(),
        n_type,
        Some(duration_ms),
    );
}

pub fn show_progress_notification(title: &str, snippet: &str, progress: f32) {
    {
        let mut active = ACTIVE_PROGRESS.lock().unwrap();
        *active = Some(ProgressNotification {
            title: title.to_string(),
            snippet: snippet.to_string(),
            progress: progress.clamp(0.0, 100.0),
        });
    }
    ensure_window_and_post(WM_APP_UPDATE_PROGRESS);
}

pub fn hide_progress_notification() {
    {
        let mut active = ACTIVE_PROGRESS.lock().unwrap();
        *active = None;
    }
    ensure_window_and_post(WM_APP_HIDE_PROGRESS);
}

pub(super) fn escape_js_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'")
        .replace('\n', " ")
}

fn ensure_window_and_post(msg: u32) {
    // Check if already warmed up
    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        // Trigger warmup if not started yet
        window::warmup();
        // We don't block anymore. The notification is in PENDING_QUEUE.
        // internal_create_window_loop will post WM_APP_PROCESS_QUEUE to itself once ready.
        return;
    }

    let hwnd_val = BADGE_HWND.load(Ordering::SeqCst);
    let hwnd = HWND(hwnd_val as *mut _);
    if hwnd_val != 0 && !hwnd.is_invalid() {
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                // Only log FAILURES - this fires on every progress tick (hundreds of
                // times during a large model download), so logging each Ok floods the
                // console with no signal.
                if let Err(e) = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) {
                    println!("[Badge] PostMessage failed: {e:?}");
                }
            } else {
                BADGE_HWND.store(0, Ordering::SeqCst);
                IS_WARMED_UP.store(false, Ordering::SeqCst);
            }
        }
    } else {
        println!("[Badge] Invalid HWND: {:?}", hwnd);
    }
}
