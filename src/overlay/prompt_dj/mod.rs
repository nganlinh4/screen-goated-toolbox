use std::sync::Once;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::component_registry::web_assets::WebAssetComponent;
mod html;
mod lifecycle;
mod scripts;
mod window;

use window::internal_create_pdj_loop;

static REGISTER_PDJ_CLASS: Once = Once::new();
const WM_APP_SHOW: u32 = WM_USER + 101;
const WM_APP_UPDATE_SETTINGS: u32 = WM_USER + 102;

// Thread-local storage for WebView
thread_local! {
    static PDJ_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    static PDJ_WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { std::cell::RefCell::new(None) };
    static PDJ_ASSET_PACK: std::cell::RefCell<Option<crate::component_registry::web_assets::WebAssetPack>> = const { std::cell::RefCell::new(None) };
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
    let capability =
        crate::runtime_support::require_webview2(crate::runtime_support::WebViewFeature::PromptDj);
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }

    crate::component_registry::web_assets::launch_when_ready(
        WebAssetComponent::PromptDj,
        show_prompt_dj_ready,
    );
}

fn show_prompt_dj_ready() {
    let action = {
        let mut state = lifecycle::STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let action = state.request_show();
        if let lifecycle::ShowAction::Show(hwnd, generation) = action {
            unsafe {
                let _ = PostMessageW(Some(hwnd.0), WM_APP_SHOW, WPARAM(generation), LPARAM(0));
            }
        }
        action
    };
    if matches!(action, lifecycle::ShowAction::Start) {
        start_window_thread();
    }
}

fn start_window_thread() {
    if let Err(error) = std::thread::Builder::new().spawn(|| {
        let _active = lifecycle::ActiveWindow {
            on_failure: report_open_failure,
        };
        unsafe {
            internal_create_pdj_loop();
        }
    }) {
        crate::log_info!("[PromptDJ] Failed to start window thread: {}", error);
        lifecycle::finish_owner(report_open_failure);
    }
}

/// Every launch failure path, including a window thread that could not start,
/// tells the user in their language instead of failing silently.
fn report_open_failure() {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::overlay::auto_copy_badge::show_error_notification(open_failure_message(&language));
}

fn open_failure_message(language: &str) -> &'static str {
    match language {
        "vi" => "Không thể mở PromptDJ. Vui lòng thử lại.",
        "ko" => "PromptDJ를 열 수 없습니다. 다시 시도해 주세요.",
        _ => "Could not open PromptDJ. Please try again.",
    }
}

pub(crate) fn are_web_assets_installed() -> bool {
    crate::component_registry::web_assets::is_installed_for_display(WebAssetComponent::PromptDj)
}

pub(crate) fn web_assets_dir() -> std::path::PathBuf {
    crate::component_registry::web_assets::component_dir(WebAssetComponent::PromptDj)
}

pub(crate) fn download_web_assets(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    crate::component_registry::web_assets::download_from_manager(
        WebAssetComponent::PromptDj,
        stop,
        use_badge,
    )
}

pub(crate) fn remove_web_assets() -> anyhow::Result<()> {
    let _removal = lifecycle::REMOVAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _removing = lifecycle::RemovalWindow::begin();
    stop_for_component_removal()
        .and_then(|()| crate::component_registry::web_assets::remove(WebAssetComponent::PromptDj))
}

fn stop_for_component_removal() -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        {
            let state = lifecycle::STATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !state.hwnd.is_invalid() {
                unsafe {
                    let _ = PostMessageW(Some(state.hwnd.0), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else if !state.active {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            anyhow::bail!("PromptDJ did not stop before component removal timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn update_settings() {
    let state = lifecycle::STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !state.hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(
                Some(state.hwnd.0),
                WM_APP_UPDATE_SETTINGS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
