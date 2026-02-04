// --- SELECTION MODULE ---
// Screen selection overlay with zoom, pan, and color picker support.

mod magnification;
mod messages;
mod render;
mod state;

use crate::win_types::SendHwnd;
use messages::{selection_hook_proc, selection_wnd_proc};
use render::sync_layered_window_contents;
use state::*;
use std::sync::atomic::Ordering;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export public items
pub use render::extract_crop_from_hbitmap_public;
pub use state::is_selection_overlay_active;

#[allow(static_mut_refs)]
pub fn show_selection_overlay(preset_idx: usize, hotkey_id: i32) {
    unsafe {
        CURRENT_PRESET_IDX = preset_idx;
        CURRENT_HOTKEY_ID = hotkey_id;
        SELECTION_OVERLAY_ACTIVE.store(true, Ordering::SeqCst);
        CURRENT_ALPHA = 0;
        IS_FADING_OUT = false;
        IS_DRAGGING = false;

        // Reset zoom state
        ZOOM_LEVEL = 1.0;
        ZOOM_CENTER_X = 0.0;
        ZOOM_CENTER_Y = 0.0;
        RENDER_ZOOM = 1.0;
        RENDER_CENTER_X = 0.0;
        RENDER_CENTER_Y = 0.0;
        IS_RIGHT_DRAGGING = false;
        ZOOM_ALPHA_OVERRIDE = None;

        // Only reset session flags if NOT already in continuous mode
        if !crate::overlay::continuous_mode::is_active() {
            HOLD_DETECTED_THIS_SESSION.store(false, Ordering::SeqCst);
            CONTINUOUS_ACTIVATED_THIS_SESSION.store(false, Ordering::SeqCst);
        }

        // Initialize Hotkey Tracking for Continuous Mode
        if let Some((mods, vk)) = crate::overlay::continuous_mode::get_current_hotkey_info() {
            TRIGGER_MODIFIERS = mods;
            TRIGGER_VK_CODE = vk;

            if !crate::overlay::continuous_mode::is_active() {
                let is_physically_held = (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0;
                IS_HOTKEY_HELD.store(is_physically_held, Ordering::SeqCst);
            }
        } else {
            IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
            TRIGGER_MODIFIERS = 0;
            TRIGGER_VK_CODE = 0;
        }

        SELECTION_ABORT_SIGNAL.store(false, Ordering::SeqCst);
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SnippingOverlay");

        let mut wc = WNDCLASSW::default();
        if !GetClassInfoW(Some(instance.into()), class_name, &mut wc).is_ok() {
            wc.lpfnWndProc = Some(selection_wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_CROSS).unwrap();
            wc.lpszClassName = class_name;
            wc.hbrBackground = CreateSolidBrush(COLORREF(0x00000000));
            RegisterClassW(&wc);
        }

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("Snipping"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        SELECTION_OVERLAY_HWND = SendHwnd(hwnd);

        // Install Hook
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(selection_hook_proc),
            Some(GetModuleHandleW(None).unwrap().into()),
            0,
        );
        if let Ok(h) = hook {
            SELECTION_HOOK = h;
        }

        // Re-check physical key state AFTER hook is installed
        if TRIGGER_VK_CODE != 0 {
            let is_still_held = (GetAsyncKeyState(TRIGGER_VK_CODE as i32) as u16 & 0x8000) != 0;
            if !is_still_held {
                IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
            }
        }

        // Initial sync to set alpha 0
        sync_layered_window_contents(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
        let _ = SetTimer(Some(hwnd), CONTINUOUS_CHECK_TIMER_ID, 50, None);

        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
                if msg.message == WM_QUIT {
                    break;
                }
            }
            if msg.message == WM_QUIT {
                break;
            }

            if SELECTION_ABORT_SIGNAL.load(Ordering::SeqCst) {
                let _ = SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                SELECTION_ABORT_SIGNAL.store(false, Ordering::SeqCst);
            }

            let _ = WaitMessage();
        }

        // Uninstall Hook
        let hook = std::ptr::addr_of!(SELECTION_HOOK).read();
        if !hook.is_invalid() {
            let _ = UnhookWindowsHookEx(hook);
            SELECTION_HOOK = HHOOK(std::ptr::null_mut());
        }

        SELECTION_OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
        SELECTION_OVERLAY_HWND = SendHwnd::default();
    }
}
