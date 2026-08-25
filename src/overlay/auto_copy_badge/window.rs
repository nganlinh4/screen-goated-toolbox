use super::messages::badge_wnd_proc;
use super::*;
use crate::win_types::HwndWrapper;
use std::sync::atomic::Ordering;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::core::*;
use wry::{Rect, WebViewBuilder};

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
        let _ = CoInitialize(None);

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
                *ctx.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                    crate::overlay::webview_runtime::Profile::Common,
                ));
            }
        });
        // Stagger start to avoid global WebView2 init lock contention
        std::thread::sleep(std::time::Duration::from_millis(50));

        let webview = {
            // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
            let init = crate::overlay::webview_init::acquire("recorder-copy-badge");
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
                let badge_html = super::html::get_badge_html();
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
                            let _ =
                                PostMessageW(Some(hwnd), WM_APP_HIDE_BADGE, WPARAM(0), LPARAM(0));
                        } else if body.starts_with("error:") {
                            crate::log_info!("[BadgeJS] {}", body);
                        }
                    })
                    .build(&wrapper)
            });

            if let Err(error) = &build_res {
                crate::log_info!("[Badge] WebView initialization failed: {error:?}");
            }
            init.finish(build_res.is_ok());
            build_res
        };

        if let Ok(wv) = webview {
            crate::overlay::webview_diagnostics::attach_webview2_close_on_failure(
                "auto-copy-badge",
                hwnd,
                &wv,
            );
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
