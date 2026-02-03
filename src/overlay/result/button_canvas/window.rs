//! Window creation for button canvas

use super::{
    html::generate_canvas_html, ipc::handle_ipc_message, wnd_proc::canvas_wnd_proc,
    CANVAS_HWND, CANVAS_WEBVIEW, CANVAS_WEB_CONTEXT, IS_WARMED_UP, REGISTER_CANVAS_CLASS,
};
use std::sync::atomic::Ordering;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

// Re-export send_windows_update from wnd_proc
pub use super::wnd_proc::send_windows_update;

// HWND wrapper for wry
struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}
impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0 .0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

/// Create the fullscreen transparent canvas window
pub fn create_canvas_window() {
    unsafe {
        // Initialize COM for WebView on this thread
        let _ = CoInitialize(None);

        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("SGTButtonCanvas");

        REGISTER_CANVAS_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(canvas_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        // Get virtual screen dimensions
        let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let v_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let v_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("ButtonCanvas"),
            WS_POPUP | WS_CLIPCHILDREN,
            v_x,
            v_y,
            v_w,
            v_h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            return;
        }

        CANVAS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Enable transparent background
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Initialize window region to empty (fully click-through)
        let empty_rgn = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(hwnd, Some(empty_rgn), true);

        // Initialize WebContext
        CANVAS_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let html = generate_canvas_html();
        let wrapper = HwndWrapper(hwnd);

        let webview = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            crate::log_info!("[ButtonCanvas] Acquired init lock. Building...");

            let build_res = CANVAS_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                    WebViewBuilder::new_with_web_context(web_ctx)
                } else {
                    WebViewBuilder::new()
                };

                let builder =
                    crate::overlay::html_components::font_manager::configure_webview(builder);

                let page_url =
                    crate::overlay::html_components::font_manager::store_html_page(html.clone())
                        .unwrap_or_else(|| {
                            format!("data:text/html,{}", urlencoding::encode(&html))
                        });

                builder
                    .with_bounds(Rect {
                        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
                            0.0, 0.0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            v_w as u32,
                            (v_h - 1) as u32,
                        )),
                    })
                    .with_transparent(true)
                    .with_visible(false)
                    .with_focused(false)
                    .with_url(&page_url)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        handle_ipc_message(msg.body());
                    })
                    .build_as_child(&wrapper)
            });
            crate::log_info!(
                "[ButtonCanvas] Build finished. Releasing lock. Status: {}",
                if build_res.is_ok() { "OK" } else { "ERR" }
            );
            build_res
        };

        match webview {
            Ok(wv) => {
                crate::log_info!("[ButtonCanvas] WebView created successfully!");
                CANVAS_WEBVIEW.with(|cell| {
                    *cell.borrow_mut() = Some(wv);
                });
                IS_WARMED_UP.store(true, Ordering::SeqCst);
                crate::log_info!("[ButtonCanvas] Canvas is now warmed up and ready");
            }
            Err(e) => {
                crate::log_info!("[ButtonCanvas] Failed to create WebView: {:?}", e);
                crate::log_info!("[ButtonCanvas] Destroying canvas window due to WebView failure");
                let _ = DestroyWindow(hwnd);
                CANVAS_HWND.store(0, Ordering::SeqCst);
                CoUninitialize();
                return;
            }
        }

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        super::IS_INITIALIZING.store(false, Ordering::SeqCst);
        CANVAS_HWND.store(0, Ordering::SeqCst);
        CANVAS_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });

        CoUninitialize();
    }
}
