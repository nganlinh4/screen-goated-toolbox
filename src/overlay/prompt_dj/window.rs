use crate::win_types::HwndWrapper;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use wry::{Rect, WebViewBuilder};

use crate::win_types::SendHwnd;

use super::{
    PDJ_ASSET_PACK, PDJ_WEB_CONTEXT, PDJ_WEBVIEW, REGISTER_PDJ_CLASS, WM_APP_SHOW,
    WM_APP_UPDATE_SETTINGS, clear_pdj_webview, html, lifecycle, scripts, with_pdj_webview,
};

fn push_settings(hwnd: HWND) {
    // Update lang and theme
    let (api_key, lang, theme_mode) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.gemini_api_key.clone(),
            app.config.ui_language.clone(),
            app.config.theme_mode.clone(),
        )
    };

    let theme_str = theme_mode.as_web_str();

    // Update window icon based on theme
    let is_dark = theme_str == "dark";
    crate::gui::utils::set_window_icon(hwnd, is_dark);

    with_pdj_webview(|webview| {
        let script = scripts::build_settings_post_message_script(&api_key, &lang, theme_str);
        let _ = webview.evaluate_script(&script);
    });
}

unsafe extern "system" fn pdj_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                let show = lifecycle::STATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .can_show(SendHwnd(hwnd));
                if !show {
                    return LRESULT(0);
                }
                push_settings(hwnd);

                if !lifecycle::STATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .can_show(SendHwnd(hwnd))
                {
                    return LRESULT(0);
                }

                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                lifecycle::STATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .acknowledge_show(SendHwnd(hwnd), wparam.0);
                LRESULT(0)
            }
            WM_APP_UPDATE_SETTINGS => {
                // Update lang and theme immediately even if hidden
                push_settings(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                crate::log_info!("[PromptDJ] close requested; destroying window");
                lifecycle::STATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .begin_close();
                with_pdj_webview(|webview| {
                    let _ = webview
                        .evaluate_script("window.postMessage({ type: 'pm-dj-stop-audio' }, '*')");
                });
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                clear_pdj_webview("WM_DESTROY");
                PDJ_ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
                let mut state = lifecycle::STATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                state.begin_close();
                state.hwnd = SendHwnd::default();
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_NCCALCSIZE => {
                if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_SIZE => {
                with_pdj_webview(|webview| {
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    let width = r.right - r.left;
                    let height = r.bottom - r.top;
                    let _ = webview.set_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            width as u32,
                            height as u32,
                        )),
                    });
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub(super) unsafe fn internal_create_pdj_loop() {
    unsafe {
        // 1. Create Window
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("PromptDJ_Class_Persistent");

        REGISTER_PDJ_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(pdj_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let area = crate::overlay::display_placement::primary_work_area();
        let screen_w = area.width;
        let screen_h = area.height;

        // Adaptive sizing based on screen aspect ratio:
        // - Width: Use 70% of screen width, capped between 1200 and 1600 pixels
        // - Height: Scales inversely with aspect ratio for consistent UI appearance
        //   - At 16:9 (1.78:1): ~72% of screen height → 775px on 1080p
        //   - At 21:9 (2.37:1): ~60% of screen height → 650px on 1080p ultrawide
        let aspect_ratio = screen_w as f64 / screen_h as f64;
        let base_aspect = 16.0 / 9.0; // 1.778
        let height_pct = (0.72 - (aspect_ratio - base_aspect) * 0.20).clamp(0.50, 0.80);

        let width = ((screen_w as f64 * 0.70) as i32).clamp(1200, 1600);
        let height = ((screen_h as f64 * height_pct) as i32).clamp(550, 900);
        let rect = area.centered(width, height);

        let (api_key, lang, theme_mode) = {
            let app = crate::APP.lock().unwrap();
            (
                app.config.gemini_api_key.clone(),
                app.config.ui_language.clone(),
                app.config.theme_mode.clone(),
            )
        };

        let title_wide = windows::core::HSTRING::from("SGT DJ");

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title_wide.as_ptr()),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU, // Start hidden (no WS_VISIBLE)
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        lifecycle::STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .hwnd = SendHwnd(hwnd);

        // Enable rounded corners
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        // Set Window Icon
        let is_dark = theme_mode.is_dark();
        crate::gui::utils::set_window_icon(hwnd, is_dark);

        // 2. Create WebView
        let wrapper = HwndWrapper(hwnd);

        let theme_str = theme_mode.as_web_str();

        // Font CSS from local HTTP server — CSS @font-face url() only works over http/https, not custom protocols

        let hwnd_ipc = hwnd;

        // Build inlined HTML and serve via the shared font server
        // so this WebView joins the shared browser process (same user data dir + origin)
        let (inlined_html, web_asset_pack) = match html::build_inlined_html() {
            Ok(assets) => assets,
            Err(error) => {
                crate::log_info!("[PromptDJ] Could not open interface assets: {error}");
                let _ = DestroyWindow(hwnd);
                return;
            }
        };
        let Some(page) =
            crate::overlay::html_components::font_manager::store_html_page(inlined_html)
                .and_then(|url| crate::overlay::trusted_page::TrustedPage::new(&url))
        else {
            crate::log_info!("[PromptDJ] Could not serve the interface page");
            let _ = DestroyWindow(hwnd);
            return;
        };
        let init_script =
            scripts::build_prompt_dj_init_script(&api_key, &lang, theme_str, page.url());

        PDJ_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                *ctx.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                    crate::overlay::webview_runtime::Profile::Common,
                ));
            }
        });

        // Brief delay to ensure window is fully initialized before creating WebView
        std::thread::sleep(std::time::Duration::from_millis(100));

        let webview_result = {
            // LOCK SCOPE: Serialized build to prevent resource contention
            let init = crate::overlay::webview_init::acquire("prompt-dj");
            crate::log_info!("[PromptDJ] Acquired init lock. Building...");

            let build_res = PDJ_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let ipc_page = page.clone();
                let navigation_page = page.clone();
                let builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap())
                    .with_initialization_script(&init_script)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        if !ipc_page.allow_ipc(&msg.uri().to_string()) {
                            return;
                        }
                        let body = msg.body().as_str();
                        if body == "drag_window" {
                            crate::overlay::utils::begin_window_drag(hwnd_ipc);
                        } else if body == "minimize_window" {
                            let _ = ShowWindow(hwnd_ipc, SW_MINIMIZE);
                        } else if body == "close_window" {
                            let _ = PostMessageW(Some(hwnd_ipc), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    })
                    .with_navigation_handler(move |url| navigation_page.allow_navigation(&url))
                    .with_url(page.url());

                builder.build_as_child(&wrapper)
            });
            crate::log_info!(
                "[PromptDJ] Build finished. Status: {}",
                if build_res.is_ok() { "OK" } else { "ERR" }
            );
            init.finish(build_res.is_ok());
            build_res
        };

        let webview = match webview_result {
            Ok(wv) => wv,
            Err(e) => {
                crate::log_info!("[PromptDJ] Failed to create WebView: {e:?}");
                let _ = DestroyWindow(hwnd);
                return;
            }
        };
        crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
            "prompt-dj",
            hwnd,
            &webview,
        );
        // Initial Resize
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let _ = webview.set_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                (r.right - r.left) as u32,
                (r.bottom - r.top) as u32,
            )),
        });

        let stored_webview = PDJ_WEBVIEW.with(|wv| match wv.try_borrow_mut() {
            Ok(mut slot) => {
                *slot = Some(webview);
                true
            }
            Err(error) => {
                crate::log_info!("[PromptDJ] failed to store WebView: {}", error);
                false
            }
        });
        if !stored_webview {
            let _ = DestroyWindow(hwnd);
            return;
        }
        PDJ_ASSET_PACK.with(|slot| *slot.borrow_mut() = Some(web_asset_pack));

        let ready = {
            let mut state = lifecycle::STATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if state.closing || state.hwnd.0 != hwnd {
                false
            } else {
                state.mark_ready();
                if state.removing {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                } else if state.has_pending_show() {
                    let _ = PostMessageW(
                        Some(hwnd),
                        WM_APP_SHOW,
                        WPARAM(state.show_generation),
                        LPARAM(0),
                    );
                }
                true
            }
        };
        if !ready {
            clear_pdj_webview("window destroyed during setup");
            PDJ_ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
            return;
        }

        // 3. Message Loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        clear_pdj_webview("message loop exit");
        PDJ_ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
    }
}
