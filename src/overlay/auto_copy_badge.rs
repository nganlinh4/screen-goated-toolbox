use crate::APP;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, Once};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

#[path = "auto_copy_badge_html.rs"]
mod html;

use self::html::get_badge_html;

static REGISTER_BADGE_CLASS: Once = Once::new();

// Thread-safe handle using atomic (like preset_wheel)
static BADGE_HWND: AtomicIsize = AtomicIsize::new(0);
static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);

// Messages
const WM_APP_PROCESS_QUEUE: u32 = WM_USER + 201;
const WM_APP_UPDATE_PROGRESS: u32 = WM_USER + 202;
const WM_APP_HIDE_PROGRESS: u32 = WM_USER + 203;

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
pub struct PendingNotification {
    pub title: String,
    pub snippet: String,
    pub n_type: NotificationType,
    pub duration_ms: Option<u32>,
}

#[derive(Clone, Debug)]
struct ProgressNotification {
    title: String,
    snippet: String,
    progress: f32,
}

lazy_static::lazy_static! {
    static ref PENDING_QUEUE: Mutex<VecDeque<PendingNotification>> = Mutex::new(VecDeque::new());
    static ref ACTIVE_PROGRESS: Mutex<Option<ProgressNotification>> = Mutex::new(None);
}

thread_local! {
    static BADGE_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    static BADGE_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

// Dimensions
const BADGE_WIDTH: i32 = 1200; // Super wide
const BADGE_HEIGHT: i32 = 400; // Taller for stacking

/// Wrapper for HWND to implement HasWindowHandle
struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0.0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

fn enqueue_notification_with_duration(
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
    let title = locale.auto_copied_badge.to_string();
    drop(app);

    let clean_text = text.replace('\n', " ").replace('\r', "");
    let snippet = format!("\"{}\"", clean_text);

    enqueue_notification(title, snippet, NotificationType::Success);
}

pub fn show_auto_copy_badge_image() {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.auto_copied_badge.to_string();
    let snippet = locale.auto_copied_image_badge.to_string();
    drop(app);

    enqueue_notification(title, snippet, NotificationType::Success);
}

pub fn show_auto_copy_badge_media_file(file_path: &str) {
    let app = APP.lock().unwrap();
    let ui_lang = app.config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    let title = locale.auto_copied_badge.to_string();
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

fn escape_js_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'")
        .replace('\n', " ")
}

fn ensure_window_and_post(msg: u32) {
    // Check if already warmed up
    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        // Trigger warmup if not started yet
        warmup();
        // We don't block anymore. The notification is in PENDING_QUEUE.
        // internal_create_window_loop will post WM_APP_PROCESS_QUEUE to itself once ready.
        return;
    }

    let hwnd_val = BADGE_HWND.load(Ordering::SeqCst);
    let hwnd = HWND(hwnd_val as *mut _);
    if hwnd_val != 0 && !hwnd.is_invalid() {
        unsafe {
            let res = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
            println!("[Badge] PostMessage Result: {:?}", res);
        }
    } else {
        println!("[Badge] Invalid HWND: {:?}", hwnd);
    }
}

pub fn warmup() {
    // Prevent multiple warmup threads from spawning (like preset_wheel)
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        internal_create_window_loop();
    });
}

fn internal_create_window_loop() {
    unsafe {
        // Initialize COM for the thread (Critical for WebView2/Wry)
        let coinit = CoInitialize(None);
        crate::log_info!("[Badge] Internal Loop Start - CoInit: {:?}", coinit);

        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("SGT_AutoCopyBadgeWebView");

        REGISTER_BADGE_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(badge_wnd_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });
        crate::log_info!("[Badge] Class Registered");

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT AutoCopy Badge"),
            WS_POPUP,
            -4000,
            -4000,
            BADGE_WIDTH,
            BADGE_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();
        crate::log_info!("[Badge] Window created with HWND: {:?}", hwnd);

        if hwnd.is_invalid() {
            crate::log_info!("[Badge] Window creation failed, HWND is invalid.");
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            BADGE_HWND.store(0, Ordering::SeqCst);
            CoUninitialize();
            return;
        }

        // Don't store HWND yet - wait until WebView is ready
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let wrapper = HwndWrapper(hwnd);

        // Initialize shared WebContext if needed (uses same data dir as other modules)
        BADGE_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                // Consolidate all minor overlays to 'common' to share one browser process and keep RAM at ~80MB
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });
        crate::log_info!("[Badge] Starting WebView initialization...");

        // Stagger start to avoid global WebView2 init lock contention
        std::thread::sleep(std::time::Duration::from_millis(50));

        let webview = {
            // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            crate::log_info!("[Badge] Acquired init lock. Building...");

            let build_res = BADGE_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                    WebViewBuilder::new_with_web_context(web_ctx)
                } else {
                    WebViewBuilder::new()
                };

                let builder =
                    crate::overlay::html_components::font_manager::configure_webview(builder);

                // Store HTML in font server and get URL for same-origin font loading
                let badge_html = get_badge_html();
                let page_url = crate::overlay::html_components::font_manager::store_html_page(
                    badge_html.clone(),
                )
                .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&badge_html)));

                builder
                    .with_transparent(true)
                    .with_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            BADGE_WIDTH as u32,
                            BADGE_HEIGHT as u32,
                        )),
                    })
                    .with_url(&page_url)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        let body = msg.body();
                        if body == "finished" {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        } else if body.starts_with("error:") {
                            crate::log_info!("[BadgeJS] {}", body);
                        }
                    })
                    .build(&wrapper)
            });

            crate::log_info!(
                "[Badge] Build phase finished. Releasing lock. Status: {}",
                if build_res.is_ok() { "OK" } else { "ERR" }
            );
            build_res
        };

        if let Ok(wv) = webview {
            crate::log_info!("[Badge] WebView initialization SUCCESSFUL");
            BADGE_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });

            // Now that WebView is ready, publicize the HWND and mark as ready
            BADGE_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            IS_WARMED_UP.store(true, Ordering::SeqCst);

            // Process any notifications that were enqueued during warmup
            let _ = PostMessageW(Some(hwnd), WM_APP_PROCESS_QUEUE, WPARAM(0), LPARAM(0));
            if ACTIVE_PROGRESS.lock().unwrap().is_some() {
                let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_PROGRESS, WPARAM(0), LPARAM(0));
            }
        } else {
            // Initialization failed - cleanup and exit
            let _ = DestroyWindow(hwnd);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            BADGE_HWND.store(0, Ordering::SeqCst);
            CoUninitialize();
            return;
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup on exit - reset all state so warmup can be retriggered
        BADGE_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });
        BADGE_HWND.store(0, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        CoUninitialize();
    }
}

unsafe extern "system" fn badge_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_PROCESS_QUEUE => {
                let app = APP.lock().unwrap();
                let is_dark = match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                };
                drop(app);

                // Update badge position (if screen changed?)
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let x = (screen_w - BADGE_WIDTH) / 2;
                let y = screen_h - BADGE_HEIGHT - 100;

                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    BADGE_WIDTH,
                    BADGE_HEIGHT,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );

                // Fetch generic queue items
                let mut items = Vec::new();
                {
                    let mut q = PENDING_QUEUE.lock().unwrap();
                    while let Some(item) = q.pop_front() {
                        items.push(item);
                    }
                }

                if !items.is_empty() {
                    BADGE_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            // Update Theme
                            let theme_script = format!("window.setTheme({});", is_dark);
                            let _ = webview.evaluate_script(&theme_script);

                            // Add Notifications logic
                            for item in items {
                                let type_str = match item.n_type {
                                    NotificationType::Success => "success",
                                    NotificationType::FileCopy => "file_copy",
                                    NotificationType::GifCopy => "gif_copy",
                                    NotificationType::Info => "info",
                                    NotificationType::Update => "update",
                                    NotificationType::Error => "error",
                                };

                                let safe_title = item.title;

                                let safe_snippet = item.snippet;
                                let duration_js = item
                                    .duration_ms
                                    .map(|ms| ms.to_string())
                                    .unwrap_or_else(|| "null".to_string());

                                let script = format!(
                                    "window.addNotification('{}', '{}', '{}', {});",
                                    escape_js_text(&safe_title),
                                    escape_js_text(&safe_snippet),
                                    type_str,
                                    duration_js
                                );
                                let _ = webview.evaluate_script(&script);
                            }
                        }
                    });
                }

                LRESULT(0)
            }
            WM_APP_UPDATE_PROGRESS => {
                let app = APP.lock().unwrap();
                let is_dark = match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                };
                drop(app);

                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let x = (screen_w - BADGE_WIDTH) / 2;
                let y = screen_h - BADGE_HEIGHT - 100;

                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    BADGE_WIDTH,
                    BADGE_HEIGHT,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );

                let progress = ACTIVE_PROGRESS.lock().unwrap().clone();
                if let Some(progress) = progress {
                    BADGE_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            let theme_script = format!("window.setTheme({});", is_dark);
                            let _ = webview.evaluate_script(&theme_script);

                            let script = format!(
                                "window.upsertProgressNotification('{}', '{}', {});",
                                escape_js_text(&progress.title),
                                escape_js_text(&progress.snippet),
                                progress.progress
                            );
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }

                LRESULT(0)
            }
            WM_APP_HIDE_PROGRESS => {
                BADGE_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.evaluate_script("window.removeProgressNotification();");
                    }
                });
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
