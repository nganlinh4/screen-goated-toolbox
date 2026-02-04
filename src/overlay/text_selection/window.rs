// --- TEXT SELECTION WINDOW ---
// Window procedure and message loop for the badge WebView.

use super::clipboard::{get_clipboard_text, keyboard_hook_proc, process_selected_text};
use super::html::{get_html, get_localized_badge_text, get_localized_image_badge_text};
use super::state::*;
use crate::overlay::realtime_webview::state::HwndWrapper;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub unsafe extern "system" fn tag_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let lang = {
        if let Ok(app) = APP.try_lock() {
            app.config.ui_language.clone()
        } else {
            "en".to_string()
        }
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match msg {
        WM_APP_SHOW => {
            crate::log_info!("[Badge] WM_APP_SHOW received");
            TEXT_BADGE_VISIBLE.store(true, Ordering::SeqCst);
            let _ = KillTimer(Some(hwnd), 1);

            // ALWAYS reposition to cursor when showing
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = MoveWindow(
                hwnd,
                pt.x + OFFSET_X,
                pt.y + OFFSET_Y,
                BADGE_WIDTH,
                BADGE_HEIGHT,
                false,
            );

            // Update badge text based on continuous mode status
            let is_continuous = crate::overlay::continuous_mode::is_active();
            let badge_text = get_localized_badge_text(&lang, is_continuous);
            crate::log_info!(
                "[Badge] WM_APP_SHOW: is_continuous={}, badge_text='{}'",
                is_continuous,
                badge_text
            );

            // Trigger Fade In Script and update text
            {
                let state = SELECTION_STATE.lock().unwrap();
                if let Some(wv) = state.webview.as_ref() {
                    let _ = wv.evaluate_script(&format!(
                        "updateState(false, '{}'); playEntry();",
                        badge_text
                    ));
                }
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            LRESULT(0)
        }
        WM_APP_HIDE => {
            crate::log_info!("[Badge] WM_APP_HIDE received");
            TEXT_BADGE_VISIBLE.store(false, Ordering::SeqCst);
            {
                let state = SELECTION_STATE.lock().unwrap();
                if let Some(wv) = state.webview.as_ref() {
                    let _ = wv.evaluate_script("playExit();");
                }
            }
            SetTimer(Some(hwnd), 1, 150, None);
            LRESULT(0)
        }
        WM_APP_SHOW_IMAGE_BADGE => {
            crate::log_info!("[Badge] WM_APP_SHOW_IMAGE_BADGE received");
            let _ = KillTimer(Some(hwnd), 2);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = MoveWindow(
                hwnd,
                pt.x + OFFSET_X,
                pt.y + OFFSET_Y,
                BADGE_WIDTH,
                BADGE_HEIGHT,
                false,
            );

            let image_badge_text = get_localized_image_badge_text(&lang);

            {
                let state = SELECTION_STATE.lock().unwrap();
                if let Some(wv) = state.webview.as_ref() {
                    if !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst) {
                        let _ = wv.evaluate_script("playExit();");
                    }
                    let _ = wv.evaluate_script(&format!(
                        "updateImageText('{}'); showImageBadge();",
                        image_badge_text
                    ));
                }
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            LRESULT(0)
        }
        WM_APP_HIDE_IMAGE_BADGE => {
            crate::log_info!("[Badge] WM_APP_HIDE_IMAGE_BADGE received");
            {
                let state = SELECTION_STATE.lock().unwrap();
                if let Some(wv) = state.webview.as_ref() {
                    let _ = wv.evaluate_script("hideImageBadge();");
                }
            }
            SetTimer(Some(hwnd), 2, 150, None);
            LRESULT(0)
        }
        WM_APP_UPDATE_CONTINUOUS => {
            crate::log_info!("[Badge] WM_APP_UPDATE_CONTINUOUS received");
            if TEXT_BADGE_VISIBLE.load(Ordering::SeqCst) {
                let continuous_text = get_localized_badge_text(&lang, true);
                crate::log_info!("[Badge] Updating text to: '{}'", continuous_text);
                {
                    let state = SELECTION_STATE.lock().unwrap();
                    if let Some(wv) = state.webview.as_ref() {
                        let _ = wv
                            .evaluate_script(&format!("updateState(false, '{}')", continuous_text));
                    }
                }
            }
            LRESULT(0)
        }
        WM_APP_RESTORE_AFTER_CAPTURE => {
            crate::log_info!("[Badge] WM_APP_RESTORE_AFTER_CAPTURE received");
            let text_visible = TEXT_BADGE_VISIBLE.load(Ordering::SeqCst);
            let image_visible = IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst);
            if text_visible || image_visible {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == 1 {
                let _ = KillTimer(Some(hwnd), 1);
                {
                    let initial_text = INITIAL_TEXT_GLOBAL.lock().unwrap();
                    reset_ui_state(&initial_text);
                }
                if !IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst)
                    && !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
                {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            } else if wparam.0 == 2 {
                let _ = KillTimer(Some(hwnd), 2);
                if !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
                    && !IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst)
                {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = KillTimer(Some(hwnd), 1);
            let initial_text = INITIAL_TEXT_GLOBAL.lock().unwrap();
            reset_ui_state(&initial_text);
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }));
    match result {
        Ok(lresult) => lresult,
        Err(_) => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn internal_create_tag_thread() {
    unsafe {
        use windows::Win32::System::Com::*;
        let _coinit = CoInitialize(None);

        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_TextTag_Web_Persistent");

        REGISTER_TAG_CLASS.call_once(|| {
            let mut wc = WNDCLASSEXW::default();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            wc.lpfnWndProc = Some(tag_wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            let _ = RegisterClassExW(&wc);
        });

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_TRANSPARENT
                | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Tag"),
            WS_POPUP,
            -1000,
            -1000,
            200,
            120,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            return;
        }

        let (initial_is_dark, lang) = {
            let app = APP.lock().unwrap();
            (
                app.config.theme_mode == crate::config::ThemeMode::Dark
                    || (app.config.theme_mode == crate::config::ThemeMode::System
                        && crate::gui::utils::is_system_in_dark_mode()),
                app.config.ui_language.clone(),
            )
        };

        let initial_text = match lang.as_str() {
            "vi" => "Bôi đen văn bản...",
            "ko" => "텍스트 선택...",
            _ => "Select text...",
        };
        *INITIAL_TEXT_GLOBAL.lock().unwrap() = initial_text.to_string();
        let html_content = get_html(initial_text);

        let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));

        SELECTION_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                *ctx.borrow_mut() = Some(wry::WebContext::new(Some(shared_data_dir)));
            }
        });

        let page_url =
            crate::overlay::html_components::font_manager::store_html_page(html_content.clone())
                .unwrap_or_else(|| {
                    format!("data:text/html,{}", urlencoding::encode(&html_content))
                });

        let mut final_webview: Option<wry::WebView> = None;

        std::thread::sleep(std::time::Duration::from_millis(150));

        for _attempt in 1..=3 {
            let res = {
                let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();

                let build_res = SELECTION_WEB_CONTEXT.with(|ctx| {
                    let mut ctx_ref = ctx.borrow_mut();
                    let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                        wry::WebViewBuilder::new_with_web_context(web_ctx)
                    } else {
                        wry::WebViewBuilder::new()
                    };

                    builder
                        .with_bounds(wry::Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                BADGE_WIDTH as u32,
                                BADGE_HEIGHT as u32,
                            )),
                        })
                        .with_url(&page_url)
                        .with_transparent(true)
                        .build_as_child(&HwndWrapper(hwnd))
                });

                build_res
            };

            match res {
                Ok(wv) => {
                    final_webview = Some(wv);
                    break;
                }
                Err(_e) => {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }

        if let Some(webview) = final_webview {
            let init_script = format!("updateTheme({});", initial_is_dark);
            let _ = webview.evaluate_script(&init_script);
            SELECTION_STATE.lock().unwrap().webview = Some(webview);
        } else {
            let _ = DestroyWindow(hwnd);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            let _ = CoUninitialize();
            return;
        }

        TAG_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        IS_WARMED_UP.store(true, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);

        if PENDING_SHOW_ON_WARMUP.swap(false, Ordering::SeqCst) {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = MoveWindow(
                hwnd,
                pt.x + OFFSET_X,
                pt.y + OFFSET_Y,
                BADGE_WIDTH,
                BADGE_HEIGHT,
                false,
            );
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }

        if IMAGE_CONTINUOUS_PENDING_SHOW.swap(false, Ordering::SeqCst) {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = MoveWindow(
                hwnd,
                pt.x + OFFSET_X,
                pt.y + OFFSET_Y,
                BADGE_WIDTH,
                BADGE_HEIGHT,
                false,
            );
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_IMAGE_BADGE, WPARAM(0), LPARAM(0));
        }

        let mut msg = MSG::default();
        let mut visible = false;
        let mut current_is_dark = initial_is_dark;
        let mut last_sent_is_selecting = false;

        loop {
            if msg.message == WM_QUIT {
                break;
            }

            if visible {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        visible = false;
                        break;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if msg.message == WM_QUIT {
                    break;
                }

                // KEY HELD SYNC (POLLING)
                if TRIGGER_VK_CODE != 0 {
                    let is_physically_down =
                        (GetAsyncKeyState(TRIGGER_VK_CODE as i32) as u16 & 0x8000) != 0;
                    if !is_physically_down && IS_HOTKEY_HELD.load(Ordering::SeqCst) {
                        IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
                    }
                }
            } else {
                if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else {
                    break;
                }
            }

            let is_actually_visible = IsWindowVisible(hwnd).as_bool();

            if is_actually_visible != visible {
                visible = is_actually_visible;
                let mut state = SELECTION_STATE.lock().unwrap();
                if visible {
                    if state.hook_handle.is_invalid() {
                        let hook = SetWindowsHookExW(
                            WH_KEYBOARD_LL,
                            Some(keyboard_hook_proc),
                            Some(GetModuleHandleW(None).unwrap().into()),
                            0,
                        );
                        if let Ok(h) = hook {
                            state.hook_handle = h;
                        }
                    }

                    last_sent_is_selecting = false;

                    let new_is_dark = crate::overlay::is_dark_mode();
                    if new_is_dark != current_is_dark {
                        current_is_dark = new_is_dark;
                        if let Some(wv) = state.webview.as_ref() {
                            let _ =
                                wv.evaluate_script(&format!("updateTheme({});", current_is_dark));
                        }
                    }

                    if let Some(wv) = state.webview.as_ref() {
                        let is_continuous = crate::overlay::continuous_mode::is_active();
                        let lang = {
                            let app = APP.lock().unwrap();
                            app.config.ui_language.clone()
                        };
                        let badge_text = get_localized_badge_text(&lang, is_continuous);
                        crate::log_info!(
                            "[Badge] Visibility transition (visible=true): is_continuous={}, badge_text='{}'",
                            is_continuous,
                            badge_text
                        );
                        let reset_js = format!("updateState(false, '{}')", badge_text);
                        let _ = wv.evaluate_script(&reset_js);
                    }
                } else {
                    if !crate::overlay::continuous_mode::is_active()
                        && !state.hook_handle.is_invalid()
                    {
                        let _ = UnhookWindowsHookEx(state.hook_handle);
                        state.hook_handle = HHOOK::default();
                    }
                }
            }

            if visible {
                if TAG_ABORT_SIGNAL.load(Ordering::SeqCst) {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    continue;
                }

                let new_is_dark = crate::overlay::is_dark_mode();
                if new_is_dark != current_is_dark {
                    current_is_dark = new_is_dark;
                    if let Some(wv) = SELECTION_STATE.lock().unwrap().webview.as_ref() {
                        let _ = wv.evaluate_script(&format!("updateTheme({});", current_is_dark));
                    }
                }

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let target_x = pt.x + OFFSET_X;
                let target_y = pt.y + OFFSET_Y;

                let _ = MoveWindow(hwnd, target_x, target_y, BADGE_WIDTH, BADGE_HEIGHT, false);

                // EARLY CONTINUOUS MODE TRIGGER
                let cm_active = crate::overlay::continuous_mode::is_active();
                let session_activated = CONTINUOUS_ACTIVATED_THIS_SESSION.load(Ordering::SeqCst);
                let image_badge_visible = IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst);

                if !cm_active && !session_activated && !image_badge_visible {
                    let heartbeat = crate::overlay::continuous_mode::was_triggered_recently(2000);
                    if heartbeat {
                        HOLD_DETECTED_THIS_SESSION.store(true, Ordering::SeqCst);
                    }

                    if HOLD_DETECTED_THIS_SESSION.load(Ordering::SeqCst) {
                        let p_idx = SELECTION_STATE.lock().unwrap().preset_idx;
                        if p_idx != usize::MAX {
                            let mut hotkey_name =
                                crate::overlay::continuous_mode::get_hotkey_name();
                            if hotkey_name.is_empty() {
                                hotkey_name =
                                    crate::overlay::continuous_mode::get_latest_hotkey_name();
                            }
                            if hotkey_name.is_empty() {
                                hotkey_name = "Hotkey".to_string();
                            }

                            let p_name = {
                                if let Ok(app) = APP.lock() {
                                    app.config
                                        .presets
                                        .get(p_idx)
                                        .map(|p| p.id.clone())
                                        .unwrap_or_default()
                                } else {
                                    "Preset".to_string()
                                }
                            };

                            if p_name != "preset_text_select_master" {
                                crate::log_info!(
                                    "[Badge] Early trigger activating global continuous mode for preset {}",
                                    p_idx
                                );
                                crate::overlay::continuous_mode::activate(
                                    p_idx,
                                    hotkey_name.clone(),
                                );
                                crate::overlay::continuous_mode::show_activation_notification(
                                    &p_name,
                                    &hotkey_name,
                                );
                                CONTINUOUS_ACTIVATED_THIS_SESSION.store(true, Ordering::SeqCst);
                                super::update_badge_for_continuous_mode();
                                let _ =
                                    PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
                            }
                        }
                    }
                }

                let lbutton_down = (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0;

                let mut should_spawn_thread = false;
                let mut preset_idx_for_thread = 0;

                let text_badge_active = TEXT_BADGE_VISIBLE.load(Ordering::SeqCst);
                let lang = {
                    if let Ok(app) = APP.try_lock() {
                        app.config.ui_language.clone()
                    } else {
                        "en".to_string()
                    }
                };

                let update_js = if text_badge_active {
                    let mut state = SELECTION_STATE.lock().unwrap();

                    if !state.is_selecting && lbutton_down {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let hwnd_under_mouse = WindowFromPoint(pt);
                        let mut pid: u32 = 0;
                        GetWindowThreadProcessId(hwnd_under_mouse, Some(&mut pid));
                        let our_pid = std::process::id();

                        let over_result_window =
                            crate::overlay::result::button_canvas::is_point_over_result_window(
                                pt.x, pt.y,
                            );

                        if pid != our_pid && !over_result_window {
                            state.is_selecting = true;
                            MOUSE_START_X.store(pt.x, Ordering::SeqCst);
                            MOUSE_START_Y.store(pt.y, Ordering::SeqCst);
                        }
                    } else if state.is_selecting && !lbutton_down && !state.is_processing {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let start_x = MOUSE_START_X.load(Ordering::SeqCst);
                        let start_y = MOUSE_START_Y.load(Ordering::SeqCst);
                        let dx = (pt.x - start_x).abs();
                        let dy = (pt.y - start_y).abs();
                        let distance = dx + dy;

                        let is_canvas_dragging =
                            crate::overlay::result::button_canvas::is_dragging();

                        let hwnd_under_mouse = WindowFromPoint(pt);
                        let mut release_pid: u32 = 0;
                        GetWindowThreadProcessId(hwnd_under_mouse, Some(&mut release_pid));
                        let our_pid = std::process::id();
                        let released_on_our_ui = release_pid == our_pid;

                        if distance >= 10 && !released_on_our_ui && !is_canvas_dragging {
                            state.is_processing = true;
                            should_spawn_thread = true;
                            preset_idx_for_thread = state.preset_idx;
                        } else {
                            state.is_selecting = false;
                        }
                    }

                    if state.is_selecting != last_sent_is_selecting {
                        last_sent_is_selecting = state.is_selecting;
                        let new_text: String = if state.is_selecting {
                            match lang.as_str() {
                                "vi" => "Thả chuột để xử lý",
                                "ko" => "처리를 위해 마우스를 놓으세요",
                                _ => "Release to process",
                            }
                            .to_string()
                        } else {
                            let is_continuous = crate::overlay::continuous_mode::is_active();
                            get_localized_badge_text(&lang, is_continuous)
                        };

                        Some(format!(
                            "updateState({}, '{}')",
                            state.is_selecting, new_text
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(js) = update_js {
                    if let Some(webview) = SELECTION_STATE.lock().unwrap().webview.as_ref() {
                        let _ = webview.evaluate_script(&js);
                    }
                }

                if should_spawn_thread {
                    let hwnd_val = hwnd.0 as usize;
                    std::thread::spawn(move || {
                        worker_thread(hwnd_val, preset_idx_for_thread);
                    });
                }

                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }

        // Cleanup
        {
            let mut state = SELECTION_STATE.lock().unwrap();
            state.webview = None;
            if !state.hook_handle.is_invalid() {
                let _ = UnhookWindowsHookEx(state.hook_handle);
                state.hook_handle = HHOOK::default();
            }
        }
    }
}

/// Worker thread for processing text selection
unsafe fn worker_thread(hwnd_val: usize, preset_idx_for_thread: usize) {
    let hwnd_copy = HWND(hwnd_val as *mut std::ffi::c_void);

    if TAG_ABORT_SIGNAL.load(Ordering::Relaxed) || !TEXT_BADGE_VISIBLE.load(Ordering::Relaxed) {
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Clear Clipboard
    if OpenClipboard(Some(HWND::default())).is_ok() {
        let _ = EmptyClipboard();
        let _ = CloseClipboard();
    }

    let send_input_event = |vk: u16, flags: KEYBD_EVENT_FLAGS| {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                    wScan: 0,
                },
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    };

    // Ctrl + C chain
    send_input_event(VK_CONTROL.0, KEYBD_EVENT_FLAGS(0));
    std::thread::sleep(std::time::Duration::from_millis(20));
    send_input_event(0x43, KEYBD_EVENT_FLAGS(0));
    std::thread::sleep(std::time::Duration::from_millis(20));
    send_input_event(0x43, KEYEVENTF_KEYUP);
    std::thread::sleep(std::time::Duration::from_millis(20));
    send_input_event(VK_CONTROL.0, KEYEVENTF_KEYUP);

    let mut clipboard_text = String::new();
    for _ in 0..10 {
        if TAG_ABORT_SIGNAL.load(Ordering::Relaxed) || !TEXT_BADGE_VISIBLE.load(Ordering::Relaxed)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        clipboard_text = get_clipboard_text();
        if !clipboard_text.is_empty() {
            break;
        }
    }

    if !clipboard_text.trim().is_empty()
        && !TAG_ABORT_SIGNAL.load(Ordering::Relaxed)
        && TEXT_BADGE_VISIBLE.load(Ordering::Relaxed)
    {
        // HIDE FIRST
        let _ = PostMessageW(Some(hwnd_copy), WM_APP_HIDE, WPARAM(0), LPARAM(0));

        let mut p_idx = preset_idx_for_thread;

        // CHECK FOR CONTINUOUS MODE ACTIVATION
        let cm_active_before = crate::overlay::continuous_mode::is_active();
        let session_flag = CONTINUOUS_ACTIVATED_THIS_SESSION.load(Ordering::SeqCst);

        if !cm_active_before && !session_flag {
            let mut held = if TRIGGER_MODIFIERS == 0 {
                IS_HOTKEY_HELD.load(Ordering::SeqCst)
            } else {
                crate::overlay::continuous_mode::are_modifiers_still_held()
            };

            if !held {
                held = crate::overlay::continuous_mode::was_triggered_recently(1500);
            }

            if held {
                let mut hotkey_name = crate::overlay::continuous_mode::get_hotkey_name();

                let dbg_latest = crate::overlay::continuous_mode::get_latest_hotkey_name();
                crate::log_info!(
                    "[TextSelection] Late Check - Persistent: '{}', Latest: '{}'",
                    hotkey_name,
                    dbg_latest
                );

                if hotkey_name.is_empty() {
                    hotkey_name = dbg_latest;
                }
                if hotkey_name.is_empty() {
                    hotkey_name = "Hotkey".to_string();
                }

                let preset_name = {
                    if let Ok(app) = APP.lock() {
                        app.config
                            .presets
                            .get(p_idx)
                            .map(|p| p.id.clone())
                            .unwrap_or_default()
                    } else {
                        "Preset".to_string()
                    }
                };

                let current_active_idx = crate::overlay::continuous_mode::get_preset_idx();
                if current_active_idx != p_idx {
                    p_idx = current_active_idx;
                }
                crate::overlay::continuous_mode::activate(p_idx, hotkey_name.clone());
                crate::overlay::continuous_mode::show_activation_notification(
                    &preset_name,
                    &hotkey_name,
                );
                CONTINUOUS_ACTIVATED_THIS_SESSION.store(true, Ordering::SeqCst);
                super::update_badge_for_continuous_mode();
            }
        }

        // CONTINUOUS MODE RETRIGGER
        let cm_active = crate::overlay::continuous_mode::is_active();
        let cm_idx = crate::overlay::continuous_mode::get_preset_idx();
        if cm_active && cm_idx == p_idx {
            let retrigger_idx = p_idx;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                if crate::overlay::continuous_mode::is_active() {
                    let _ = super::show_text_selection_tag(retrigger_idx);
                }
            });
        }

        process_selected_text(p_idx, clipboard_text);
    }

    // Always reset state at the end
    let mut state = SELECTION_STATE.lock().unwrap();
    state.is_selecting = false;
    state.is_processing = false;
}
