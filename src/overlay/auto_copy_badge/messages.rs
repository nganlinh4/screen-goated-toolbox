use super::*;
use wry::WebView;

/// Reposition the badge window to bottom-center of the primary screen and show it.
/// Shared by the queue-processing and progress-update handlers.
fn show_badge_centered(hwnd: HWND) {
    unsafe {
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
    }
}

/// Push the current theme into the badge WebView.
fn push_theme(webview: &WebView, is_dark: bool) {
    let theme_script = format!("window.setTheme({});", is_dark);
    let _ = webview.evaluate_script(&theme_script);
}

pub(super) unsafe extern "system" fn badge_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_PROCESS_QUEUE => {
                let app = APP.lock().unwrap();
                let is_dark = app.config.theme_mode.is_dark();
                drop(app);

                show_badge_centered(hwnd);

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
                            push_theme(webview, is_dark);

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
                let is_dark = app.config.theme_mode.is_dark();
                drop(app);

                show_badge_centered(hwnd);

                let progress = ACTIVE_PROGRESS.lock().unwrap().clone();
                if let Some(progress) = progress {
                    BADGE_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            push_theme(webview, is_dark);

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
            WM_APP_HIDE_BADGE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
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
