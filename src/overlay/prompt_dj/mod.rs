use std::sync::Once;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::win_types::SendHwnd;

mod html;
mod scripts;
mod volume;
mod window;

use window::internal_create_pdj_loop;

static REGISTER_PDJ_CLASS: Once = Once::new();
static mut PDJ_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
static mut IS_WARMED_UP: bool = false;
static mut IS_INITIALIZING: bool = false;
const WM_APP_SHOW: u32 = WM_USER + 101;
const WM_APP_UPDATE_SETTINGS: u32 = WM_USER + 102;

// Thread-local storage for WebView
thread_local! {
    static PDJ_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    static PDJ_WEB_CONTEXT: std::cell::RefCell<Option<wry::WebContext>> = const { std::cell::RefCell::new(None) };
}

fn with_pdj_webview(action: impl FnOnce(&wry::WebView)) {
    PDJ_WEBVIEW.with(|cell| match cell.try_borrow() {
        Ok(webview_slot) => {
            if let Some(webview) = webview_slot.as_ref() {
                action(webview);
            }
        }
        Err(error) => {
            crate::log_info!("[PromptDJ] skipped re-entrant WebView access: {}", error);
        }
    });
}

fn clear_pdj_webview(reason: &str) {
    PDJ_WEBVIEW.with(|cell| match cell.try_borrow_mut() {
        Ok(mut webview_slot) => {
            *webview_slot = None;
        }
        Err(error) => {
            crate::log_info!(
                "[PromptDJ] deferred WebView clear during {}: {}",
                reason,
                error
            );
        }
    });
}

pub fn show_prompt_dj() {
    let capability = crate::runtime_support::require_webview2("Prompt DJ");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }

    unsafe {
        // Initialize on-demand if not warmed up
        if !IS_WARMED_UP {
            if !IS_INITIALIZING {
                IS_INITIALIZING = true;
                std::thread::spawn(|| {
                    internal_create_pdj_loop();
                });
            }

            // Polling thread to auto-show once ready
            std::thread::spawn(|| {
                // Poll for 10 seconds (100 * 100ms)
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let hwnd_wrapper = std::ptr::addr_of!(PDJ_HWND).read();
                    if IS_WARMED_UP && !hwnd_wrapper.is_invalid() {
                        let _ =
                            PostMessageW(Some(hwnd_wrapper.0), WM_APP_SHOW, WPARAM(0), LPARAM(0));
                        return;
                    }
                }
            });
            return;
        }

        let hwnd_wrapper = std::ptr::addr_of!(PDJ_HWND).read();
        if !hwnd_wrapper.is_invalid() {
            let _ = PostMessageW(Some(hwnd_wrapper.0), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn update_settings() {
    unsafe {
        if !std::ptr::addr_of!(PDJ_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(PDJ_HWND.0),
                WM_APP_UPDATE_SETTINGS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
