//! Orb overlay window thread: creates the `WS_EX_NOREDIRECTIONBITMAP` /
//! DirectComposition window (capture-excluded, transparent, click-through), builds the
//! composition-hosted WebView2, hands it to the window proc, and pumps the message loop
//! until the window is destroyed. Mirrors the `result::button_canvas` lifecycle but uses
//! DComp hosting (see `dcomp.rs`) instead of wry, which can't host WebView2 in
//! composition mode.

use std::sync::atomic::Ordering;

use windows061::Win32::Foundation::HWND;
use windows061::Win32::Graphics::Gdi::HBRUSH;
use windows061::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows061::Win32::System::LibraryLoader::GetModuleHandleW;
use windows061::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, IDC_ARROW, LoadCursorW, MSG,
    RegisterClassW, SetWindowDisplayAffinity, TranslateMessage, WDA_EXCLUDEFROMCAPTURE, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows061::core::w;

use super::{
    ORB_COMP, ORB_HWND, ORB_INITIALIZING, ORB_PAGE_READY, ORB_WARMED_UP, ORB_WEBVIEW,
    REGISTER_ORB_CLASS, dcomp::build_host, virtual_screen, wnd_proc::orb_wnd_proc,
};

/// Thread entry: bring up the orb window + WebView2 host and run its message loop.
pub(super) fn create_orb_window() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let hwnd = match create_window() {
            Some(h) => h,
            None => {
                ORB_INITIALIZING.store(false, Ordering::SeqCst);
                CoUninitialize();
                return;
            }
        };
        ORB_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // CRITICAL: hide the orb from the agent's own screen capture (GDI BitBlt / PrintWindow /
        // DWM). It still renders on screen for the user. Valid because the window is
        // WS_EX_NOREDIRECTIONBITMAP + DirectComposition (NOT WS_EX_LAYERED). Win10 2004+ (build
        // 19041); no-op below that.
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);

        let host = match build_host(hwnd) {
            Ok(h) => h,
            Err(e) => {
                crate::log_info!("[CCOrb] DComp host build failed: {e:?}");
                let _ = DestroyWindow(hwnd);
                ORB_HWND.store(0, Ordering::SeqCst);
                ORB_INITIALIZING.store(false, Ordering::SeqCst);
                CoUninitialize();
                return;
            }
        };

        // Hand the WebView + composition controller to the window proc (same thread): one drives
        // `ExecuteScript`, the other forwards mouse input + the cursor.
        ORB_WEBVIEW.with(|c| *c.borrow_mut() = Some(host.webview.clone()));
        ORB_COMP.with(|c| *c.borrow_mut() = Some(host.comp.clone()));
        ORB_WARMED_UP.store(true, Ordering::SeqCst);

        // The window stays hidden until `show_orb` posts WM_APP_SHOW_ORB (after the page reports
        // `orbReady`), so the first reveal paints the transparent canvas, never a white flash.

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        drop(host);
        ORB_WARMED_UP.store(false, Ordering::SeqCst);
        ORB_PAGE_READY.store(false, Ordering::SeqCst);
        ORB_INITIALIZING.store(false, Ordering::SeqCst);
        ORB_HWND.store(0, Ordering::SeqCst);
        ORB_WEBVIEW.with(|c| *c.borrow_mut() = None);
        ORB_COMP.with(|c| *c.borrow_mut() = None);
        CoUninitialize();
    }
}

/// Create the hidden, fullscreen, transparent, capture-ready host window.
unsafe fn create_window() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class_name = w!("SGTComputerControlOrbDComp");
        REGISTER_ORB_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(orb_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        let (vx, vy, vw, vh) = virtual_screen();
        // WS_EX_NOREDIRECTIONBITMAP: no GDI redirection surface → composed purely via
        // DirectComposition (true per-pixel alpha), compatible with WDA capture-exclusion.
        // No WS_VISIBLE: the window starts hidden until `show_orb`.
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("CCOrb"),
            WS_POPUP,
            vx,
            vy,
            vw,
            vh,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()
    }
}
