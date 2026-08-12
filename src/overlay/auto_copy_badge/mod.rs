use crate::APP;
#[cfg(feature = "recorder-worker")]
use std::cell::RefCell;
#[cfg(feature = "recorder-worker")]
use std::collections::VecDeque;
#[cfg(feature = "recorder-worker")]
use std::sync::Once;
#[cfg(feature = "recorder-worker")]
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
#[cfg(feature = "recorder-worker")]
use windows::Win32::Foundation::*;
#[cfg(feature = "recorder-worker")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(feature = "recorder-worker")]
use wry::{WebContext, WebView};

#[path = "../auto_copy_badge_html.rs"]
mod html;
#[cfg(feature = "recorder-worker")]
mod messages;
mod progress;
#[cfg(feature = "recorder-worker")]
mod window;

pub use progress::DownloadProgressBadge;

#[cfg(feature = "recorder-worker")]
pub(super) static REGISTER_BADGE_CLASS: Once = Once::new();

// Thread-safe handle using atomic (like preset_wheel)
#[cfg(feature = "recorder-worker")]
pub(super) static BADGE_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(feature = "recorder-worker")]
pub(super) static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "recorder-worker")]
pub(super) static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);

// Messages
#[cfg(feature = "recorder-worker")]
pub(super) const WM_APP_PROCESS_QUEUE: u32 = WM_USER + 201;
#[cfg(feature = "recorder-worker")]
pub(super) const WM_APP_UPDATE_PROGRESS: u32 = WM_USER + 202;
#[cfg(feature = "recorder-worker")]
pub(super) const WM_APP_HIDE_PROGRESS: u32 = WM_USER + 203;
#[cfg(feature = "recorder-worker")]
pub(super) const WM_APP_HIDE_BADGE: u32 = WM_USER + 204;
#[cfg(feature = "recorder-worker")]
pub(super) const WM_APP_UPDATE_THEME: u32 = WM_USER + 205;

/// Notification themes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotificationType {
    Success, // Green - auto copied
    #[cfg(feature = "recorder-worker")]
    FileCopy, // Cyan - copied media file
    #[cfg(feature = "recorder-worker")]
    GifCopy, // Pink - copied GIF file
    Info,    // Yellow - loading/warming up
    #[cfg(not(feature = "recorder-worker"))]
    Update, // Blue - update available (longer duration)
    Error,   // Red - error (e.g., no writable area for auto-paste)
}

#[cfg(feature = "recorder-worker")]
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

#[cfg(feature = "recorder-worker")]
pub(super) static PENDING_QUEUE: LazyLock<Mutex<VecDeque<PendingNotification>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
pub(super) static ACTIVE_PROGRESS: LazyLock<Mutex<Option<ProgressNotification>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(feature = "recorder-worker")]
thread_local! {
    pub(super) static BADGE_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    pub(super) static BADGE_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

// Dimensions
#[cfg(feature = "recorder-worker")]
pub(super) const BADGE_WIDTH: i32 = 1200; // Super wide
#[cfg(feature = "recorder-worker")]
pub(super) const BADGE_HEIGHT: i32 = 400; // Taller for stacking

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn document() -> String {
    html::get_badge_html()
}

pub fn locale_text() -> crate::gui::locale::BadgeLocaleText {
    let ui_language = APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language).badge
}

pub fn format_locale(template: &str, replacements: &[(&str, &str)]) -> String {
    replacements
        .iter()
        .fold(template.to_string(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

#[cfg(not(feature = "recorder-worker"))]
pub fn update_theme(is_dark: bool) {
    crate::overlay::status_compositor::update_theme(is_dark);
}

pub fn enqueue_notification_with_duration(
    title: String,
    snippet: String,
    n_type: NotificationType,
    duration_ms: Option<u32>,
) {
    #[cfg(not(feature = "recorder-worker"))]
    {
        crate::overlay::status_compositor::add_notification(
            title,
            snippet,
            notification_kind(n_type),
            duration_ms,
        );
    }
    #[cfg(feature = "recorder-worker")]
    {
        let mut q = PENDING_QUEUE.lock().unwrap();
        q.push_back(PendingNotification {
            title,
            snippet,
            n_type,
            duration_ms,
        });
    }
    #[cfg(feature = "recorder-worker")]
    ensure_window_and_post(WM_APP_PROCESS_QUEUE);
}

#[cfg(not(feature = "recorder-worker"))]
fn notification_kind(notification: NotificationType) -> &'static str {
    match notification {
        NotificationType::Success => "success",
        NotificationType::Info => "info",
        NotificationType::Update => "update",
        NotificationType::Error => "error",
    }
}

fn enqueue_notification(title: String, snippet: String, n_type: NotificationType) {
    enqueue_notification_with_duration(title, snippet, n_type, None);
}

#[cfg(not(feature = "recorder-worker"))]
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

#[cfg(not(feature = "recorder-worker"))]
pub fn show_auto_copy_badge_image() {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.shell.auto_copied_badge.to_string();
    let snippet = locale.shell.auto_copied_image_badge.to_string();
    drop(app);

    enqueue_notification(title, snippet, NotificationType::Success);
}

#[cfg(feature = "recorder-worker")]
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
#[cfg(not(feature = "recorder-worker"))]
pub fn show_notification(title: &str) {
    enqueue_notification(title.to_string(), String::new(), NotificationType::Info);
}

/// Show an update available notification (blue theme, longer duration)
#[cfg(not(feature = "recorder-worker"))]
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

#[cfg(feature = "recorder-worker")]
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

fn show_progress_notification(title: &str, snippet: &str, progress: f32) {
    {
        let mut active = ACTIVE_PROGRESS.lock().unwrap();
        *active = Some(ProgressNotification {
            title: title.to_string(),
            snippet: snippet.to_string(),
            progress: progress.clamp(0.0, 100.0),
        });
    }
    #[cfg(not(feature = "recorder-worker"))]
    crate::overlay::status_compositor::progress_upsert(
        title.to_string(),
        snippet.to_string(),
        progress,
    );
    #[cfg(feature = "recorder-worker")]
    ensure_window_and_post(WM_APP_UPDATE_PROGRESS);
}

fn update_progress_notification_if_owned(title: &str, snippet: &str, progress: f32) {
    let updated = {
        let mut active = ACTIVE_PROGRESS.lock().unwrap();
        let Some(current) = active.as_mut() else {
            return;
        };
        if !progress_is_owned(Some(current), title) {
            false
        } else {
            current.snippet = snippet.to_string();
            current.progress = progress.clamp(0.0, 100.0);
            true
        }
    };
    if updated {
        #[cfg(not(feature = "recorder-worker"))]
        crate::overlay::status_compositor::progress_upsert(
            title.to_string(),
            snippet.to_string(),
            progress,
        );
        #[cfg(feature = "recorder-worker")]
        ensure_window_and_post(WM_APP_UPDATE_PROGRESS);
    }
}

/// Hide only the progress notification owned by `title`. A completed download
/// must not dismiss a newer concurrent download's badge.
fn hide_progress_notification_for(title: &str) {
    let removed = {
        let mut active = ACTIVE_PROGRESS.lock().unwrap();
        if progress_is_owned(active.as_ref(), title) {
            *active = None;
            true
        } else {
            false
        }
    };
    if removed {
        #[cfg(not(feature = "recorder-worker"))]
        crate::overlay::status_compositor::progress_remove();
        #[cfg(feature = "recorder-worker")]
        ensure_window_and_post(WM_APP_HIDE_PROGRESS);
    }
}

fn progress_is_owned(active: Option<&ProgressNotification>, title: &str) -> bool {
    active.is_some_and(|progress| progress.title == title)
}

#[cfg(feature = "recorder-worker")]
pub(super) fn escape_js_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'")
        .replace('\n', " ")
}

#[cfg(feature = "recorder-worker")]
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

#[cfg(test)]
mod progress_tests {
    use super::{ProgressNotification, progress_is_owned};

    #[test]
    fn a_completed_download_cannot_hide_a_newer_progress_owner() {
        let active = ProgressNotification {
            title: "new download".to_string(),
            snippet: "working".to_string(),
            progress: 25.0,
        };
        assert!(progress_is_owned(Some(&active), "new download"));
        assert!(!progress_is_owned(Some(&active), "old download"));
        assert!(!progress_is_owned(None, "new download"));
    }
}
