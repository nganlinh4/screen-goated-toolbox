// use crate::win_types::SendHwnd; // Removed
use crate::APP;
use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering},
    Arc, Mutex, Once,
};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

// --- GLOBAL SIGNALS (Preserving existing logic usage) ---
lazy_static::lazy_static! {
    pub static ref AUDIO_STOP_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_PAUSE_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_ABORT_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_WARMUP_COMPLETE: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    static ref VISUALIZATION_BUFFER: Mutex<[f32; 40]> = Mutex::new([0.0; 40]);
}

static LAST_SHOW_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub static CURRENT_RMS: AtomicU32 = AtomicU32::new(0);

pub fn update_audio_viz(rms: f32) {
    let bits = rms.to_bits();
    CURRENT_RMS.store(bits, Ordering::Relaxed);
}

// --- STATE MANAGEMENT ---
// 0=Not Created, 1=Hidden/Warmup, 2=Visible/Recording
static RECORDING_STATE: AtomicI32 = AtomicI32::new(0);
static RECORDING_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
static REGISTER_RECORDING_CLASS: Once = Once::new();

thread_local! {
    static RECORDING_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
    static RECORDING_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);
}

// --- CONSTANTS ---
const UI_WIDTH: i32 = 360;
const UI_HEIGHT: i32 = 140;

const WM_APP_SHOW: u32 = WM_USER + 20;
const WM_APP_HIDE: u32 = WM_USER + 21;
const WM_APP_REAL_SHOW: u32 = WM_USER + 22;
const WM_APP_UPDATE_STATE: u32 = WM_USER + 23;

// --- PUBLIC API ---

pub fn is_recording_overlay_active() -> bool {
    RECORDING_STATE.load(Ordering::SeqCst) == 2
}

pub fn stop_recording_and_submit() {
    // Check if we are already active
    if is_recording_overlay_active() {
        let was_stopped = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);

        // If already stopped (processing) or aborted, hitting this again should FORCE CLOSE
        if was_stopped {
            AUDIO_ABORT_SIGNAL.store(true, Ordering::SeqCst);
            let hwnd_val = RECORDING_HWND_VAL.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                let hwnd = HWND(hwnd_val as *mut _);
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                }
            }
        } else {
            // First time: Just stop and let it process
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            // Force update UI to "Processing"
            let hwnd_val = RECORDING_HWND_VAL.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                let hwnd = HWND(hwnd_val as *mut _);
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_STATE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

pub fn warmup_recording_overlay() {
    // Transition 0 -> 1
    if RECORDING_STATE
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::thread::spawn(|| {
            internal_create_recording_window();
        });
    }
}

pub fn show_recording_overlay(preset_idx: usize) {
    // Check current state
    let mut current = RECORDING_STATE.load(Ordering::SeqCst);

    // If state is 0, start the thread and waiting loop
    if current == 0 {
        warmup_recording_overlay();
        // Spin briefly to let thread start
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            current = RECORDING_STATE.load(Ordering::SeqCst);
            if current != 0 {
                break;
            }
        }
    }

    // Now we expect state to be 1 (Hidden) or 2 (already visible? technically we shouldn't show if visible, but let's handle re-trigger)
    // Wait for HWND
    let mut hwnd_val = RECORDING_HWND_VAL.load(Ordering::SeqCst);

    // Safety wait for HWND creation if still 0
    if hwnd_val == 0 {
        for _ in 0..50 {
            // ~1 second timeout
            std::thread::sleep(std::time::Duration::from_millis(20));
            hwnd_val = RECORDING_HWND_VAL.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                break;
            }
        }
    }

    if hwnd_val != 0 {
        // Reset Signals
        AUDIO_STOP_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_ABORT_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
        CURRENT_RMS.store(0, Ordering::Relaxed);

        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut _)),
                WM_APP_SHOW,
                WPARAM(preset_idx),
                LPARAM(0),
            );
        }
    } else {
        println!("Error: Failed to initialize recording window");
    }
}

// --- INTERNAL IMPLEMENTATION ---

struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}
impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0 .0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

fn internal_create_recording_window() {
    unsafe {
        let _ = CoInitialize(None); // Required for WebView
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_Recording_Persistent");

        REGISTER_RECORDING_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(recording_wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            RegisterClassW(&wc);
        });

        // Create window OFF-SCREEN initially (-4000, -4000)
        // WS_POPUP | WS_VISIBLE (so WebView renders) but off-screen.
        // Using Layered window for transparency
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Recording Web"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            UI_WIDTH,
            UI_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        RECORDING_HWND_VAL.store(hwnd.0 as isize, Ordering::SeqCst);

        // Windows 11 Rounded Corners - Disable native rounding to hide native border/shadow
        // We rely on CSS for rounded corners + transparency
        let corner_pref = 1u32; // DWMWCP_DONOTROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        // Glass Frame Extension (critical for per-pixel alpha with WebView)
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);

        // --- WEBVIEW CREATION ---
        let wrapper = HwndWrapper(hwnd);
        let html = generate_html();

        RECORDING_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir();
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let ipc_hwnd_val = hwnd.0 as usize;
        let webview_res = RECORDING_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let mut builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };

            builder = crate::overlay::html_components::font_manager::configure_webview(builder);

            builder
                .with_bounds(Rect {
                    position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        UI_WIDTH as u32,
                        UI_HEIGHT as u32,
                    )),
                })
                .with_transparent(true)
                .with_background_color((0, 0, 0, 0)) // Fully transparent background
                .with_html(&html)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    let hwnd = HWND(ipc_hwnd_val as *mut std::ffi::c_void);
                    let body = msg.body().as_str();
                    match body {
                        "pause_toggle" => {
                            let paused = AUDIO_PAUSE_SIGNAL.load(Ordering::SeqCst);
                            AUDIO_PAUSE_SIGNAL.store(!paused, Ordering::SeqCst);
                        }
                        "cancel" | "close" => {
                            AUDIO_ABORT_SIGNAL.store(true, Ordering::SeqCst);
                            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
                            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                        }
                        "ready" => {
                            // Handshake: WebView is ready (from resetState), so now we can REAL_SHOW
                            // Kill fallback timer 99
                            let _ = KillTimer(Some(hwnd), 99);
                            // Add a tiny delay to ensure paint catch-up
                            let _ = SetTimer(Some(hwnd), 2, 20, None);
                        }
                        "drag_window" => {
                            unsafe {
                                let _ = ReleaseCapture();
                                let _ = PostMessageW(
                                    Some(hwnd),
                                    WM_NCLBUTTONDOWN,
                                    WPARAM(2 as usize), // HTCAPTION = 2
                                    LPARAM(0 as isize),
                                );
                            }
                        }
                        _ => {}
                    }
                })
                .build(&wrapper)
        });

        if let Ok(wv) = webview_res {
            RECORDING_WEBVIEW.with(|cell| *cell.borrow_mut() = Some(wv));

            // Setup Global Key Hook for ESC (This needs to be persistent or installed/uninstalled on show/hide)
            // Better to install once and check `is_recording_overlay_active()` inside hook.
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(recording_hook_proc),
                Some(GetModuleHandleW(None).unwrap().into()),
                0,
            );

            // Message Loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            if let Ok(h) = hook {
                UnhookWindowsHookEx(h);
            }
        }

        // Cleanup on FULL EXIT
        RECORDING_WEBVIEW.with(|cell| *cell.borrow_mut() = None);
        RECORDING_STATE.store(0, Ordering::SeqCst);

        let _ = CoUninitialize();
    }
}

fn start_audio_thread(hwnd: HWND, preset_idx: usize) {
    let preset = APP.lock().unwrap().config.presets[preset_idx].clone();
    let hwnd_val = hwnd.0 as usize;

    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        crate::api::record_audio_and_transcribe(
            preset,
            AUDIO_STOP_SIGNAL.clone(),
            AUDIO_PAUSE_SIGNAL.clone(),
            AUDIO_ABORT_SIGNAL.clone(),
            hwnd,
        );
    });
}

unsafe extern "system" fn recording_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_SHOW => {
            // 1. Prepare Content (while still off-screen)
            let preset_idx = wparam.0;

            // Reset JS state
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    let _ = wv.evaluate_script("resetState();");
                }
            });

            // 2. Start Audio Logic
            start_audio_thread(hwnd, preset_idx);

            // 3. Mark state as Active (Visible)
            RECORDING_STATE.store(2, Ordering::SeqCst);

            // 5. Fallback Timer (99) - If IPC ready signal doesn't come in 500ms, show anyway
            SetTimer(Some(hwnd), 99, 500, None);

            // 5. REMOVED Timer 2 here. We now confirm via IPC "ready" signal to trigger the show.
            // This ensures we never show a blank window on first load.

            // Record Show Time to prevent race with old threads closing
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            LAST_SHOW_TIME.store(now, Ordering::SeqCst);

            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == 2 {
                // REAL SHOW TIMER (from IPC "ready")
                KillTimer(Some(hwnd), 2);
                let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
            } else if wparam.0 == 99 {
                // FALLBACK TIMER (IPC timed out)
                KillTimer(Some(hwnd), 99);
                println!("Warning: Recording overlay IPC timed out, forcing show");
                let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
            } else if wparam.0 == 1 {
                // VIZ UPDATE TIMER
                let is_processing = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
                let is_paused = AUDIO_PAUSE_SIGNAL.load(Ordering::SeqCst);
                let warming_up = !AUDIO_WARMUP_COMPLETE.load(Ordering::SeqCst);

                let rms_bits = CURRENT_RMS.load(Ordering::Relaxed);
                let rms = f32::from_bits(rms_bits);

                let state_str = if is_processing {
                    "processing"
                } else if is_paused {
                    "paused"
                } else if warming_up {
                    "warmup"
                } else {
                    "recording"
                };

                let script = format!("updateState('{}', {});", state_str, rms);

                RECORDING_WEBVIEW.with(|cell| {
                    if let Some(wv) = cell.borrow().as_ref() {
                        let _ = wv.evaluate_script(&script);
                    }
                });
            }
            LRESULT(0)
        }

        WM_APP_REAL_SHOW => {
            // Move to Center Screen
            let screen_x = GetSystemMetrics(SM_CXSCREEN);
            let screen_y = GetSystemMetrics(SM_CYSCREEN);
            let center_x = (screen_x - UI_WIDTH) / 2;
            let center_y = (screen_y - UI_HEIGHT) / 2 + 100;

            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                center_x,
                center_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            // Set Foreground/Focus
            // let _ = SetForegroundWindow(hwnd);
            // let _ = SetFocus(Some(hwnd));

            // Start Visualization Updates NOW that we are visible and ready
            // Only needing one timer start here
            let _ = SetTimer(Some(hwnd), 1, 16, None);

            // Trigger Fade In - window is now in position
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    let _ = wv.evaluate_script(
                        "setTimeout(() => document.body.classList.add('visible'), 50);",
                    );
                }
            });

            LRESULT(0)
        }

        WM_APP_HIDE => {
            // Stop logic
            let _ = KillTimer(Some(hwnd), 1);
            let _ = KillTimer(Some(hwnd), 2);
            let _ = KillTimer(Some(hwnd), 99);

            // Reset opacity immediately so it's ready for next time
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    // Use hideState() which DOES NOT trigger 'ready' signal
                    // This prevents the recursion where hide -> reset -> ready -> show -> hide
                    let _ = wv.evaluate_script("hideState();");
                }
            });

            // Move Off-screen
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                -4000,
                -4000,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );

            RECORDING_STATE.store(1, Ordering::SeqCst); // Back to Warmup/Hidden

            LRESULT(0)
        }

        WM_APP_UPDATE_STATE => {
            // Just force an immediate update cycle if needed (e.g. for processing state)
            // Timer handles this mostly, but this can be used for instant response
            LRESULT(0)
        }

        WM_CLOSE => {
            // "Close" means Hide in this persistent model
            // LOGIC FIX: Check if this is a 'stale' close from a previous thread
            let is_stop = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
            let is_abort = AUDIO_ABORT_SIGNAL.load(Ordering::SeqCst);

            if is_stop || is_abort {
                // User requested stop/abort, so this close is valid
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
            } else {
                // Natural close (error or finish?)
                // Check if we JUST started. If so, it's likely the old thread dying.
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let last = LAST_SHOW_TIME.load(Ordering::SeqCst);
                if now > last && (now - last) < 2000 {
                    // Ignore Close during first 2 seconds if not explicitly stopped
                    // This prevents race condition where previous aborted thread sends WM_CLOSE late
                } else {
                    let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }

        WM_USER_FULL_CLOSE => {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn recording_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
            if kbd.vkCode == VK_ESCAPE.0 as u32 {
                if is_recording_overlay_active() {
                    stop_recording_and_submit();
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

const WM_USER_FULL_CLOSE: u32 = WM_USER + 99;

// --- HTML GENERATOR ---
fn generate_html() -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let (text_rec, text_proc, text_wait, subtext) = {
        let app = APP.lock().unwrap();
        let lang = app.config.ui_language.as_str();

        match lang {
            "vi" => (
                "Đang ghi âm...",
                "Đang xử lý...",
                "Chuẩn bị...",
                "Nhấn ESC để dừng",
            ),
            "ko" => ("녹음 중...", "처리 중...", "준비 중...", "중지하려면 ESC"),
            _ => (
                "Recording...",
                "Processing...",
                "Starting...",
                "Press ESC to Stop",
            ),
        }
    };

    format!(
        r#"
<!DOCTYPE html>
<html>
<head>
<style>
    {font_css}
    
    * {{ box-sizing: border-box; user-select: none; }}
    
    body {{
        margin: 0;
        padding: 0;
        width: 100vw;
        height: 100vh;
        overflow: hidden;
        background: transparent;
        display: flex;
        justify-content: center;
        align-items: center;
        opacity: 0;
        transition: opacity 0.15s ease-out; 
    }}
    
    body.visible {{
        opacity: 1;
    }}

    .container {{
        width: {width}px;
        height: {height}px;
        background: rgba(18, 18, 18, 0.85);
        backdrop-filter: blur(20px);
        -webkit-backdrop-filter: blur(20px);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 20px;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        position: relative;
        box-shadow: 0 10px 30px rgba(0,0,0,0.5);
        color: white;
        font-family: 'Google Sans Flex', sans-serif;
    }}

    .status-text {{
        font-size: 18px;
        font-weight: 600;
        margin-bottom: 4px;
        text-shadow: 0 1px 2px rgba(0,0,0,0.3);
    }}
    
    .sub-text {{
        font-size: 12px;
        color: rgba(255,255,255,0.6);
        margin-bottom: 12px;
    }}

    /* Volume Canvas Styling */
    #volume-canvas {{
        height: 24px;
        width: 120px;
        border-radius: 2px;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 3px;
        margin-bottom: 5px;
    }}

    /* Waveform Animation */
    .wave-line {{
        width: 4px;
        height: 100%;
        background: linear-gradient(180deg, #00C6FF 0%, #0072FF 100%);
        border-radius: 10px;
        transform-box: fill-box;
        transform-origin: center;
        transform: scaleY(0.2);
        transition: transform 0.05s linear;
        box-shadow: 0 0 8px rgba(0, 198, 255, 0.6);
    }}
    
    .dancing .wave-line {{
        animation: wave-animation 1.2s ease-in-out infinite;
    }}

    .wave-line.delay-1 {{ animation-delay: 0s; }}
    .wave-line.delay-2 {{ animation-delay: 0.15s; }}
    .wave-line.delay-3 {{ animation-delay: 0.3s; }}
    .wave-line.delay-4 {{ animation-delay: 0.1s; }}

    @keyframes wave-animation {{
        0%, 100% {{ transform: scaleY(0.3); }}
        50% {{ transform: scaleY(0.8); }}
    }}

    .controls {{
        position: absolute;
        width: 100%;
        height: 100%;
        top: 0;
        left: 0;
        pointer-events: none;
    }}

    .btn {{
        position: absolute;
        top: 50%;
        transform: translateY(-50%);
        width: 40px;
        height: 40px;
        border-radius: 50%;
        background: rgba(255,255,255,0.05);
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        pointer-events: auto;
        transition: background 0.2s, transform 0.1s;
        color: rgba(255,255,255,0.8);
    }}
    
    .btn:hover {{
        background: rgba(255,255,255,0.15);
    }}
    .btn:active {{
        transform: translateY(-50%) scale(0.95);
    }}

    .btn-close {{ right: 15px; }}
    .btn-pause {{ left: 15px; }}
    
    .btn svg {{
        width: 20px;
        height: 20px;
        fill: currentColor;
    }}
    
    .hidden {{ display: none; }}

</style>
</head>
<body>
    <div class="container">
        
        <div class="status-text" id="status">{tx_rec}</div>
        <div class="sub-text">{tx_sub}</div>
        
        <div id="volume-canvas">
             <div class="wave-line delay-1"></div>
             <div class="wave-line delay-2"></div>
             <div class="wave-line delay-3"></div>
             <div class="wave-line delay-4"></div>
             <div class="wave-line delay-1"></div>
             <div class="wave-line delay-2"></div>
             <div class="wave-line delay-3"></div>
             <div class="wave-line delay-4"></div>
             <div class="wave-line delay-1"></div>
        </div>

        <div class="controls">
            <div class="btn btn-pause" onclick="togglePause()" id="btn-pause">
                 <svg id="icon-pause" viewBox="0 0 24 24"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>
                 <svg id="icon-play" class="hidden" viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>
            </div>
            
            <div class="btn btn-close" onclick="closeApp()">
                <svg viewBox="0 0 24 24"><path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/></svg>
            </div>
        </div>
    </div>

<script>
    const statusEl = document.getElementById('status');
    const pauseBtn = document.getElementById('btn-pause');
    const iconPause = document.getElementById('icon-pause');
    const iconPlay = document.getElementById('icon-play');
    const volumeCanvas = document.getElementById('volume-canvas');
    const bars = document.querySelectorAll('.wave-line');
    
    let currentState = "warmup"; 
    
    function updateState(state, rms) {{
        currentState = state;
        
        if (state === 'processing') {{
             statusEl.innerText = "{tx_proc}";
             volumeCanvas.classList.add('dancing');
             pauseBtn.style.display = 'none';
        }} else if (state === 'paused') {{
             statusEl.innerText = "Paused";
             volumeCanvas.classList.add('dancing');
             pauseBtn.style.display = 'flex';
             iconPause.classList.add('hidden');
             iconPlay.classList.remove('hidden');
        }} else if (state === 'warmup') {{
             statusEl.innerText = "{tx_wait}";
             volumeCanvas.classList.add('dancing');
        }} else {{
             statusEl.innerText = "{tx_rec}";
             volumeCanvas.classList.remove('dancing');
             pauseBtn.style.display = 'flex';
             iconPause.classList.remove('hidden');
             iconPlay.classList.add('hidden');
        }}
        
        if (state === 'recording') {{
             let amp = Math.min(rms * 15.0, 1.0);
             bars.forEach((bar, i) => {{
                 let factor = 1.0;
                 if (i === 0 || i === 8) factor = 0.6;
                 if (i === 1 || i === 7) factor = 0.8;
                 let h = 0.2 + (amp * factor * 1.5);
                 if (h > 1.8) h = 1.8;
                 bar.style.transform = `scaleY(${{h}})`;
             }});
        }}
    }}

    function togglePause() {{
        window.ipc.postMessage('pause_toggle');
    }}
    
    function closeApp() {{
        window.ipc.postMessage('cancel');
    }}
    
    function resetState() {{
        hideState();
        // Small timeout to allow DOM to process class removal before signaling ready
        // This ensures the opacity transition (to 0) is registered
        setTimeout(() => {{
             window.ipc.postMessage('ready');
        }}, 10);
    }}

    // Drag Logic
    const container = document.querySelector('.container');
    container.addEventListener('mousedown', (e) => {{
        // Prevent dragging if clicking buttons
        if (e.target.closest('.btn')) return;
        window.ipc.postMessage('drag_window');
    }});

    function hideState() {{
        document.body.classList.remove('visible');
    }}

</script>
</body>
</html>
    "#,
        width = UI_WIDTH - 20,
        height = UI_HEIGHT - 20,
        tx_rec = text_rec,
        tx_proc = text_proc,
        tx_wait = text_wait,
        tx_sub = subtext
    )
}
