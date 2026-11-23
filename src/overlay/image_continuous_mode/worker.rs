//! Owns the window, hooks, and rendering resources for one invocation.
use super::*;

#[derive(Default)]
struct Resources {
    hwnd: Option<HWND>,
    mouse: Option<HHOOK>,
    keyboard: Option<HHOOK>,
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if let Some(hook) = self.mouse {
                let _ = UnhookWindowsHookEx(hook);
            }
            if let Some(hook) = self.keyboard {
                let _ = UnhookWindowsHookEx(hook);
            }
            if let Some(hwnd) = self.hwnd {
                let _ = DestroyWindow(hwnd);
            }
            logic::reset_magnification();
            *GESTURE_CAPTURE.lock().unwrap_or_else(|p| p.into_inner()) = None;
            crate::overlay::cached_dib::with(&OVERLAY_DIB, |dib| dib.release());
            LAST_RENDERED_SELECTION = None;
            LAST_RENDERED_DIM_ALPHA = 0;
            RECT_OVERLAY_HWND.store(0, Ordering::SeqCst);
            DRAG_RENDER_PENDING.store(false, Ordering::SeqCst);
            OVERLAY_THREAD_ID.store(0, Ordering::SeqCst);
        }
    }
}

fn live(id: u64) -> bool {
    LIFECYCLE.lock().unwrap().live(id)
}

pub(super) fn run(id: u64, request: lifecycle::Request) -> Result<(), &'static str> {
    let mut resources = Resources::default();
    unsafe {
        // Force creation of the thread's message queue before it is published.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        let thread_id = GetCurrentThreadId();
        {
            let mut state = LIFECYCLE.lock().unwrap();
            if !state.register_thread(id, thread_id) {
                return Ok(());
            }
            OVERLAY_THREAD_ID.store(thread_id, Ordering::SeqCst);
        }
        *CAPTURE_TARGET.lock().unwrap() = request.target;
        TRIGGER_ID.store(request.hotkey_id, Ordering::SeqCst);
        TRIGGER_VK.store(request.trigger_vk, Ordering::SeqCst);
        *HOTKEY_NAME.lock().unwrap() = request.hotkey_name.clone();
        HAS_RELEASED_SINCE_ACTIVATION.store(false, Ordering::SeqCst);
        GESTURE.reset();
        DRAG_RENDER_PENDING.store(false, Ordering::SeqCst);
        *ZOOM_LEVEL.lock().unwrap() = 1.0;
        *GESTURE_CAPTURE.lock().unwrap() = None;
        CURRENT_DIM_ALPHA = 0;
        LAST_RENDERED_SELECTION = None;
        LAST_RENDERED_DIM_ALPHA = 0;
        if !live(id) {
            return Ok(());
        }

        let instance = GetModuleHandleW(None).map_err(|_| "module")?;
        let class_name = windows::core::w!("SGT_ImageContinuousOverlay");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(graphics::rect_overlay_wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_CROSS).map_err(|_| "cursor")?,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        if !live(id) {
            return Ok(());
        }
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name,
            windows::core::w!(""),
            WS_POPUP,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|_| "window")?;
        resources.hwnd = Some(hwnd);
        RECT_OVERLAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        if !live(id) {
            return Ok(());
        }
        graphics::sync_rect_overlay(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if !live(id) {
            return Ok(());
        }
        resources.mouse = Some(
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(hooks::mouse_hook_proc),
                Some(instance.into()),
                0,
            )
            .map_err(|_| "mouse hook")?,
        );
        if !live(id) {
            return Ok(());
        }
        resources.keyboard = Some(
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(hooks::keyboard_hook_proc),
                Some(instance.into()),
                0,
            )
            .map_err(|_| "keyboard hook")?,
        );
        {
            let mut state = LIFECYCLE.lock().unwrap();
            if !state.ready(id) {
                return Ok(());
            }
            RUNNING.store(true, Ordering::SeqCst);
            crate::overlay::text_selection::set_image_continuous_badge(true);
        }
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let display_name = request.target.localized_name(&language);
        if !display_name.is_empty() && live(id) {
            crate::overlay::continuous_mode::show_image_continuous_notification(
                &display_name,
                &request.hotkey_name,
            );
        }
        loop {
            if !live(id) {
                break;
            }
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result == -1 {
                return Err("message loop");
            }
            if result == 0 || !live(id) {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
