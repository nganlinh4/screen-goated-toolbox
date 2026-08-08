use super::html::DOCUMENT;
use super::protocol::{ChildEvent, HostCommand, SceneCard};
use crate::win_types::HwndWrapper;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;
use wry::http::Response;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

const WM_DRAIN_COMMANDS: u32 = WM_APP + 91;
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
static RENDERER_READY: AtomicBool = AtomicBool::new(false);
static COMMANDS: LazyLock<Mutex<VecDeque<HostCommand>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
static CARDS: LazyLock<Mutex<HashMap<isize, SceneCard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static STDOUT: LazyLock<Mutex<std::io::Stdout>> = LazyLock::new(|| Mutex::new(std::io::stdout()));

thread_local! {
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    static WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

pub fn run() -> anyhow::Result<()> {
    crate::initialization::init_com_and_dpi();

    let hwnd = create_host_window()?;
    HOST_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    start_input_thread();

    let webview = create_webview(hwnd)?;
    WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));

    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        WEBVIEW.with(|slot| *slot.borrow_mut() = None);
        WEB_CONTEXT.with(|slot| *slot.borrow_mut() = None);
        HOST_HWND.store(0, Ordering::SeqCst);
        CoUninitialize();
    }
    Ok(())
}

fn create_host_window() -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTResultSceneCompositor");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let _ = RegisterClassW(&window_class);

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("Result compositor"),
            WS_POPUP | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
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
        DwmExtendFrameIntoClientArea(hwnd, &margins)?;
        let empty = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(hwnd, Some(empty), false);
        Ok(hwnd)
    }
}

fn create_webview(hwnd: HWND) -> anyhow::Result<WebView> {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) } as u32;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) } as u32;
    let data_dir = crate::paths::app_sgt_dir()
        .join("webview_data")
        .join("result-compositor");
    let wrapper = HwndWrapper(hwnd);
    WEB_CONTEXT.with(|slot| {
        *slot.borrow_mut() = Some(WebContext::new(Some(data_dir)));
        let mut context = slot.borrow_mut();
        WebViewBuilder::new_with_web_context(context.as_mut().unwrap())
            .with_bounds(Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(width, height)),
            })
            .with_transparent(true)
            .with_focused(false)
            .with_custom_protocol("sgtresult".to_string(), |_id, request| {
                let path = request.uri().path();
                if path == "/font.ttf" {
                    return compositor_response(
                        200,
                        "font/ttf",
                        Cow::Borrowed(crate::assets::GOOGLE_SANS_FLEX),
                    );
                }
                if path == "/" || path == "/index.html" {
                    let html = DOCUMENT.replace("__SGT_FONT_VERSION__", env!("CARGO_PKG_VERSION"));
                    return compositor_response(
                        200,
                        "text/html; charset=utf-8",
                        Cow::Owned(html.into_bytes()),
                    );
                }
                compositor_response(404, "text/plain", Cow::Borrowed(b"Not Found"))
            })
            .with_url("sgtresult://localhost/index.html")
            .with_ipc_handler(|request: wry::http::Request<String>| {
                handle_renderer_event(request.body());
            })
            .build_as_child(&wrapper)
            .map_err(Into::into)
    })
}

fn compositor_response(
    status: u16,
    mime: &'static str,
    body: Cow<'static, [u8]>,
) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(body)
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"Internal Error")))
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
            COMMANDS.lock().unwrap().push_back(command);
            let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                        WM_DRAIN_COMMANDS,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }
        let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
        if hwnd_value != 0 {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    });
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
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
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
    loop {
        let command = COMMANDS.lock().unwrap().pop_front();
        let Some(command) = command else {
            break;
        };
        if command == HostCommand::Shutdown {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        apply_scene_state(&command);
        if let Ok(command_json) = serde_json::to_string(&command) {
            let script = format!("window.applyHostCommand({command_json});");
            WEBVIEW.with(|slot| {
                if let Some(webview) = slot.borrow().as_ref()
                    && let Err(error) = webview.evaluate_script(&script)
                {
                    emit_event(ChildEvent::CommandError {
                        command: command_name(&command).to_string(),
                        id: command_id(&command),
                        error: error.to_string(),
                    });
                }
            });
        }
    }
    update_window_region(hwnd);
}

fn handle_renderer_event(body: &str) {
    match body {
        "renderer_ready" => {
            RENDERER_READY.store(true, Ordering::SeqCst);
            emit_event(ChildEvent::Ready);
            let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                        WM_DRAIN_COMMANDS,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }
        "renderer_heartbeat" => emit_event(ChildEvent::Heartbeat),
        _ => {
            if let Ok(event) = serde_json::from_str::<ChildEvent>(body) {
                match event {
                    ChildEvent::Navigation { .. }
                    | ChildEvent::FitDiagnostic { .. }
                    | ChildEvent::CardDiagnostic { .. }
                    | ChildEvent::FontReady { .. }
                    | ChildEvent::CommandError { .. } => {
                        emit_event(event);
                    }
                    ChildEvent::Ready | ChildEvent::Heartbeat => {}
                }
            }
        }
    }
}

fn command_name(command: &HostCommand) -> &'static str {
    match command {
        HostCommand::Snapshot { .. } => "snapshot",
        HostCommand::Upsert { .. } => "upsert",
        HostCommand::Stream { .. } => "stream",
        HostCommand::Finalize { .. } => "finalize",
        HostCommand::Geometry { .. } => "geometry",
        HostCommand::Remove { .. } => "remove",
        HostCommand::NavigateBack { .. } => "navigate_back",
        HostCommand::NavigateForward { .. } => "navigate_forward",
        HostCommand::Shutdown => "shutdown",
    }
}

fn command_id(command: &HostCommand) -> Option<isize> {
    match command {
        HostCommand::Upsert { card } => Some(card.id),
        HostCommand::Stream { card } => Some(card.id),
        HostCommand::Finalize { card } => Some(card.id),
        HostCommand::Remove { id }
        | HostCommand::NavigateBack { id }
        | HostCommand::NavigateForward { id } => Some(*id),
        HostCommand::Snapshot { .. } | HostCommand::Geometry { .. } | HostCommand::Shutdown => None,
    }
}

fn apply_scene_state(command: &HostCommand) {
    let mut cards = CARDS.lock().unwrap();
    match command {
        HostCommand::Snapshot { cards: snapshot } => {
            cards.clear();
            cards.extend(snapshot.iter().cloned().map(|card| (card.id, card)));
        }
        HostCommand::Upsert { card } => {
            cards.insert(card.id, card.clone());
        }
        HostCommand::Stream { card: update } => {
            if let Some(card) = cards.get_mut(&update.id) {
                card.background.clone_from(&update.background);
                card.opacity = update.opacity;
                card.visible = update.visible;
                card.streaming = true;
            }
        }
        HostCommand::Finalize { card: update } => {
            if let Some(card) = cards.get_mut(&update.id) {
                card.html.clone_from(&update.html);
                card.background.clone_from(&update.background);
                card.opacity = update.opacity;
                card.visible = update.visible;
                card.streaming = false;
            }
        }
        HostCommand::Geometry { cards: updates } => {
            for update in updates {
                if let Some(card) = cards.get_mut(&update.id) {
                    card.rect = update.rect.clone();
                    card.visible = update.visible;
                }
            }
        }
        HostCommand::Remove { id } => {
            cards.remove(id);
        }
        HostCommand::NavigateBack { .. }
        | HostCommand::NavigateForward { .. }
        | HostCommand::Shutdown => {}
    }
}

fn update_window_region(hwnd: HWND) {
    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        let cards = CARDS.lock().unwrap();
        let mut visible_count = 0usize;
        for card in cards.values().filter(|card| card.visible) {
            visible_count += 1;
            let rect = CreateRectRgn(
                card.rect.x,
                card.rect.y,
                card.rect.x + card.rect.width,
                card.rect.y + card.rect.height,
            );
            let _ = CombineRgn(Some(combined), Some(combined), Some(rect), RGN_OR);
            let _ = DeleteObject(rect.into());
        }
        drop(cards);
        let _ = SetWindowRgn(hwnd, Some(combined), true);
        if visible_count == 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn resize_host(hwnd: HWND) {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
        WEBVIEW.with(|slot| {
            if let Some(webview) = slot.borrow().as_ref() {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        width as u32,
                        height as u32,
                    )),
                });
            }
        });
    }
}

fn emit_event(event: ChildEvent) {
    if let Ok(line) = serde_json::to_string(&event) {
        let mut stdout = STDOUT.lock().unwrap();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}
