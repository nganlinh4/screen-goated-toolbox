use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::{CreateRectRgn, HBRUSH, SetWindowRgn};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{ChildEvent, HostCommand};

const WM_DRAIN_COMMANDS: u32 = WM_APP + 620;

static COMMANDS: LazyLock<Mutex<CommandBuffer>> =
    LazyLock::new(|| Mutex::new(CommandBuffer::default()));
static OUTPUT: LazyLock<Mutex<std::io::Stdout>> = LazyLock::new(|| Mutex::new(std::io::stdout()));
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
static RENDERER_READY: AtomicBool = AtomicBool::new(false);
static REGISTER_CLASS: std::sync::Once = std::sync::Once::new();

pub(super) fn run() -> anyhow::Result<()> {
    unsafe {
        let _ = CoInitialize(None);
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTRealtimeCompositorHost");
        REGISTER_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Realtime Compositor"),
            WS_POPUP | WS_CLIPCHILDREN,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let empty = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(hwnd, Some(empty), false);
        HOST_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        start_input_thread();
        super::webview::create_realtime_webview(
            hwnd,
            "device",
            "English",
            "google-gtx",
            "gemini",
            16,
        )?;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        HOST_HWND.store(0, Ordering::SeqCst);
        RENDERER_READY.store(false, Ordering::SeqCst);
        super::webview::destroy_realtime_webview(hwnd);
        CoUninitialize();
    }
    Ok(())
}

fn start_input_thread() {
    std::thread::spawn(|| {
        for line in BufReader::new(std::io::stdin())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(command) = serde_json::from_str::<HostCommand>(&line) else {
                continue;
            };
            match COMMANDS.lock().unwrap().push(command) {
                PushResult::Queued => post_host(WM_DRAIN_COMMANDS),
                PushResult::Overflowed => emit_event(ChildEvent::ResyncRequested),
                PushResult::AwaitingSnapshot => {}
            }
        }
        post_host(WM_CLOSE);
    });
}

pub(super) fn renderer_ready() {
    if RENDERER_READY.swap(true, Ordering::SeqCst) {
        return;
    }
    emit_event(ChildEvent::Ready);
    post_host(WM_DRAIN_COMMANDS);
    std::thread::spawn(|| {
        while HOST_HWND.load(Ordering::SeqCst) != 0 {
            std::thread::sleep(Duration::from_secs(1));
            if HOST_HWND.load(Ordering::SeqCst) != 0 {
                emit_event(ChildEvent::Heartbeat);
            }
        }
    });
}

pub(super) fn emit_event(event: ChildEvent) {
    let Ok(mut output) = OUTPUT.lock() else {
        return;
    };
    if serde_json::to_writer(&mut *output, &event).is_ok() {
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }
}

fn post_host(message: u32) {
    let value = HOST_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(value as *mut std::ffi::c_void)),
            message,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_DRAIN_COMMANDS => {
                drain_commands(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                resize_host(hwnd);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(if super::text_input_focus::is_active() {
                MA_ACTIVATE
            } else {
                MA_NOACTIVATE
            } as isize),
            WM_CLOSE => {
                super::text_input_focus::end(hwnd);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                super::text_input_focus::end(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn drain_commands(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) {
        return;
    }
    for command in COMMANDS.lock().unwrap().drain() {
        if matches!(command, HostCommand::Shutdown) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        super::webview::apply_command(hwnd, &command);
    }
}

fn resize_host(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            SWP_NOACTIVATE,
        );
    }
    super::webview::resize_to_virtual_desktop(hwnd);
    super::webview::sync_compositor_layout(hwnd);
}
